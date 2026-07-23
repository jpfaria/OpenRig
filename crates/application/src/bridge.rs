//! `Send` bridge between an async transport (MCP/gRPC) and the `!Send`
//! `LocalDispatcher`. The transport thread `submit`s a `Command`; the
//! frontend thread `drain`s and dispatches on its own thread, replying
//! over a `futures` oneshot. No tokio runtime is pulled into this crate.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use anyhow::Result;
use futures::channel::oneshot;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;

/// Result of one dispatched command: `Ok(events)` or a stringified error
/// (the bridge crosses a thread boundary; the transport's serialization
/// layer wants an owned, `Send` payload, not `anyhow::Error`).
pub type DispatchOutcome = Result<Vec<Event>, String>;

struct BridgeRequest {
    cmd: Command,
    reply: oneshot::Sender<DispatchOutcome>,
}

/// Cloneable, `Send` handle held by the transport (MCP server thread).
#[derive(Clone)]
pub struct CommandBridge {
    tx: Sender<BridgeRequest>,
    qtx: Sender<QueryRequest>,
}

impl CommandBridge {
    /// Queue a command. Returns a oneshot receiver that resolves once the
    /// frontend drains and dispatches it. Never blocks.
    pub fn submit(&self, cmd: Command) -> oneshot::Receiver<DispatchOutcome> {
        let (reply, rx) = oneshot::channel();
        // If the frontend is gone the receiver simply never resolves; the
        // transport layer applies its own request timeout.
        let _ = self.tx.send(BridgeRequest { cmd, reply });
        rx
    }
}

/// Read-only state a transport can request. Resolved on the frontend thread
/// (which owns the `!Send` `Project`); serialization is done by domain code,
/// never re-derived in the adapter.
#[derive(Clone, Debug)]
pub enum QueryKind {
    /// Whole project as YAML.
    ProjectYaml,
    /// Available audio devices, one per line.
    Devices,
    /// Human-readable chain/block ID listing (for `midi-map.yaml` authors
    /// and the MCP `openrig://ids` resource). See [`crate::query::list_ids`].
    Ids,
    /// Per-chain input/output peak meters (`(chain_id, in_dbfs, out_dbfs)`,
    /// one record per line). Same numbers the GUI's IN/OUT bars read —
    /// every transport gets the same view (`openrig-code-quality` lei).
    ChainMeters,
    /// #554: the named preset bank of one chain (`rig:<input>`) as JSON.
    /// Resolved from the in-memory `RigProject.inputs[input].bank` — the
    /// disk-side preset library is a separate concept (different
    /// follow-up). Lets MCP / gRPC clients see the same preset list the
    /// GUI shows in the chain title combobox.
    ListChainPresets { chain: domain::ids::ChainId },
    /// #554 follow-up: every preset name in the project's in-memory
    /// `RigProject.presets` pool as JSON. A preset can sit in the pool
    /// without being bound to any input bank; tone-builder Step 0
    /// reads this to avoid silently overwriting an existing preset.
    ListProjectPresets,
    /// #561 (expanded scope): full plugin catalog as a JSON listing.
    /// See [`crate::query::list_plugin_catalog`]. Read parity for the
    /// reload / load / unload Commands so any transport can show the
    /// agent / user what is currently addressable.
    ListPluginCatalog,
    /// #561 (expanded scope): single plugin by manifest id, or
    /// `{"plugin": null}` when absent. See [`crate::query::get_plugin`].
    GetPlugin { id: String },
    /// #561 (expanded scope): text search across the catalog
    /// (case-insensitive substring on id / display_name / brand).
    /// Empty query = all entries. See [`crate::query::find_plugins`].
    FindPlugins { query: String },
    /// #572: full parameter schema for one plugin (catalog-level). No
    /// placed instance required. Resolved via
    /// `project::block::schema_for_block_model` and wrapped under a
    /// `params` envelope by [`crate::query::get_plugin_params`].
    /// Unknown id → `{"params": null}`.
    GetPluginParams { plugin_id: String },
    /// #572: list of materialised `BlockParameterDescriptor` for one
    /// placed block instance (schema + `current_value` per parameter).
    /// Resolved by [`crate::query::get_block_params`], which delegates
    /// to `AudioBlock::parameter_descriptors()` (same helper the GUI
    /// uses). Unknown chain / block → `Err`.
    GetBlockParams {
        chain: domain::ids::ChainId,
        block: domain::ids::BlockId,
    },
    /// #582: effective resolved system paths (data root + every
    /// configurable directory) as a JSON envelope. Resolved by
    /// [`crate::query::paths::resolved_paths_json`] over
    /// `AppConfig.paths` from `FilesystemStorage::load_app_config`.
    /// Every field reports the absolute resolved path — `None`
    /// overrides fall back to the OS default (consumers don't
    /// re-implement the fallback). MCP serves this as `openrig://paths`.
    Paths,
    /// #791: objective audio-quality report for one chain (THD+N, noise floor,
    /// level, dynamic range, clipping). Derived from the snapshot's chain by
    /// running the synthetic battery through the offline render, so it resolves
    /// off-frontend. MCP serves this as `openrig://chains/{chain}/quality`.
    ChainQualityReport { chain: domain::ids::ChainId },
}

struct QueryRequest {
    kind: QueryKind,
    reply: oneshot::Sender<Result<String, String>>,
}

impl CommandBridge {
    /// Read-only query, served API-style (#693): kinds derivable from
    /// the published [`crate::snapshot`] (or from process-global
    /// catalogs) resolve INLINE on the caller's thread — concurrent,
    /// never queued behind the frontend tick. Only runtime-coupled
    /// kinds (`Devices`, `ChainMeters`) still queue for the frontend,
    /// as does everything before the first snapshot exists.
    pub fn query(&self, kind: QueryKind) -> oneshot::Receiver<Result<String, String>> {
        if let Some(result) = Self::resolve_off_frontend(&kind) {
            let (reply, rx) = oneshot::channel();
            let _ = reply.send(result);
            return rx;
        }
        let (reply, rx) = oneshot::channel();
        let _ = self.qtx.send(QueryRequest { kind, reply });
        rx
    }

    /// Resolve a query without the frontend, when possible. `None` ⇒
    /// the kind needs live runtime/GUI state (or no snapshot yet) and
    /// falls back to the frontend queue.
    fn resolve_off_frontend(kind: &QueryKind) -> Option<Result<String, String>> {
        use crate::query as q;
        // Catalog / filesystem kinds never touch dispatcher state.
        match kind {
            QueryKind::ListPluginCatalog => return Some(Ok(q::list_plugin_catalog())),
            QueryKind::GetPlugin { id } => return Some(Ok(q::get_plugin(id))),
            QueryKind::FindPlugins { query } => return Some(Ok(q::find_plugins(query))),
            QueryKind::GetPluginParams { plugin_id } => {
                return Some(Ok(q::get_plugin_params(plugin_id)))
            }
            QueryKind::Paths => return Some(Ok(q::resolved_paths_json())),
            _ => {}
        }
        let snap = crate::snapshot::latest()?;
        match kind {
            QueryKind::ProjectYaml => {
                Some(infra_yaml::serialize_project(&snap.project).map_err(|e| e.to_string()))
            }
            QueryKind::Ids => Some(Ok(q::list_ids(&snap.project))),
            QueryKind::ListChainPresets { chain } => Some(match &snap.rig {
                Some(rig) => q::list_chain_presets(rig, chain),
                None => Err("no rig attached to the session".to_string()),
            }),
            QueryKind::ListProjectPresets => Some(match &snap.rig {
                Some(rig) => Ok(q::list_project_presets(rig)),
                None => Err("no rig attached to the session".to_string()),
            }),
            QueryKind::GetBlockParams { chain, block } => {
                Some(q::get_block_params(&snap.project, chain, block))
            }
            QueryKind::ChainQualityReport { chain } => Some(
                crate::query_chain_quality::chain_quality_report(&snap.project, chain),
            ),
            // Live runtime / GUI-coupled reads keep the frontend path.
            QueryKind::Devices | QueryKind::ChainMeters => None,
            // Handled above; unreachable here.
            QueryKind::ListPluginCatalog
            | QueryKind::GetPlugin { .. }
            | QueryKind::FindPlugins { .. }
            | QueryKind::GetPluginParams { .. }
            | QueryKind::Paths => None,
        }
    }
}

/// Receiver side, owned by the frontend thread.
pub struct BridgeDrain {
    rx: Receiver<BridgeRequest>,
    qrx: Receiver<QueryRequest>,
}

impl BridgeDrain {
    /// Dispatch up to `cap` queued commands on the calling (frontend) thread.
    /// Returns the events every dispatched command produced, in order, so the
    /// caller (the GUI's MIDI/MCP drain timer) can run the same screen/runtime
    /// refresh a GUI click does — a footswitch must move the screen too.
    /// Non-blocking; safe to call every tick. Empty result ⇒ nothing changed.
    pub fn drain(&self, dispatcher: &dyn CommandDispatcher, cap: usize) -> Vec<Event> {
        let mut events = Vec::new();
        let mut handled = 0;
        while handled < cap {
            match self.rx.try_recv() {
                Ok(req) => {
                    let outcome = dispatcher.dispatch(req.cmd).map_err(|e| e.to_string());
                    if let Ok(produced) = &outcome {
                        events.extend(produced.iter().cloned());
                    }
                    let _ = req.reply.send(outcome);
                    handled += 1;
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        events
    }

    /// Service queued read-only queries on the calling (frontend) thread.
    /// `resolver` runs with the frontend's `Project` access and returns the
    /// serialized payload (or an error message) for each [`QueryKind`].
    pub fn serve_queries<F>(&self, resolver: F, cap: usize) -> usize
    where
        F: Fn(&QueryKind) -> Result<String, String>,
    {
        let mut handled = 0;
        while handled < cap {
            match self.qrx.try_recv() {
                Ok(req) => {
                    let _ = req.reply.send(resolver(&req.kind));
                    handled += 1;
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        handled
    }
}

/// Create a connected `(transport handle, frontend drain)` pair.
pub fn channel() -> (CommandBridge, BridgeDrain) {
    let (tx, rx) = mpsc::channel();
    let (qtx, qrx) = mpsc::channel();
    (CommandBridge { tx, qtx }, BridgeDrain { rx, qrx })
}

/// Broadcast sink for fanned-out event batches (GUI- and MCP-originated).
///
/// Wired by [`crate::publishing_dispatcher::PublishingDispatcher`]; consumed
/// by the MCP server to emit notifications for *every* state change, no
/// matter which transport originated it.
#[derive(Clone)]
pub struct EventSink {
    tx: Sender<Vec<Event>>,
}

impl EventSink {
    /// Fan a non-empty event batch out to the stream. Never blocks.
    pub fn publish(&self, events: &[Event]) {
        if !events.is_empty() {
            let _ = self.tx.send(events.to_vec());
        }
    }
}

/// Receiver of fanned-out events, owned by the MCP server side.
pub struct EventStreamRx {
    rx: Receiver<Vec<Event>>,
}

impl EventStreamRx {
    /// Next available event batch, if any. Non-blocking.
    pub fn try_recv(&mut self) -> Option<Vec<Event>> {
        self.rx.try_recv().ok()
    }
}

/// Create a connected `(sink, receiver)` pair for event fan-out.
pub fn event_sink() -> (EventSink, EventStreamRx) {
    let (tx, rx) = mpsc::channel();
    (EventSink { tx }, EventStreamRx { rx })
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;
