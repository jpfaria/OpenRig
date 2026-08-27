//! Responsibility: carries a command from an async transport to the thread that owns the project.
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
use crate::read::NO_RIG_ATTACHED;

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

pub use crate::event_sink::{event_sink, EventSink, EventStreamRx};
pub use crate::query_kind::QueryKind;

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
                None => Err(NO_RIG_ATTACHED.to_string()),
            }),
            QueryKind::ListProjectPresets => Some(match &snap.rig {
                Some(rig) => Ok(q::list_project_presets(rig)),
                None => Err(NO_RIG_ATTACHED.to_string()),
            }),
            QueryKind::GetBlockParams { chain, block } => {
                Some(q::get_block_params(&snap.project, chain, block))
            }
            QueryKind::ChainQualityReport { chain } => Some(
                crate::query_chain_quality::chain_quality_report(&snap.project, chain),
            ),
            // Live runtime / GUI-coupled reads keep the frontend path.
            // #791/#323: runtime/GUI-coupled reads (incl. loopers) live in
            // dispatcher/runtime state, so they queue on the frontend path.
            QueryKind::Devices
            | QueryKind::ChainMeters
            | QueryKind::ChainLoopers { .. }
            | QueryKind::TunerReadings
            | QueryKind::SpectrumReadings
            | QueryKind::DiLoopState
            | QueryKind::MetronomeState
            | QueryKind::ChainLatency { .. }
            | QueryKind::ChainToneReport { .. } => None,
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

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;
