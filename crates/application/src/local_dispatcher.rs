//! `LocalDispatcher` — in-process implementation of `CommandDispatcher`.
//!
//! Holds the project via `Rc<RefCell<Project>>` for interior mutability so
//! `dispatch` can take `&self` (required by the trait; callers may hold
//! multiple references to the same dispatcher or to the same project).
//!
//! `adapter-gui`'s `ProjectSession` shares its project handle with this
//! dispatcher so both sides always see the same `Project` data with no extra
//! sync step.
//!
//! **Current state (Phase 1 skeleton):** every `Command` arm except
//! `ToggleBlockEnabled` is `unimplemented!("phase-1 task pending")`.  This is
//! intentional — no production caller dispatches those arms yet because
//! adapter-gui migration is ongoing.  Tasks 4..N will fill the arms one by
//! one, each accompanied by its own failing test that drives the
//! implementation (TDD).
//!
//! `unimplemented!()` is acceptable here because the arms are unreachable
//! from production code in this state.  The forbidden pattern is
//! `unimplemented!()` on arms that live callers can reach.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use anyhow::Result;

use domain::ids::{BlockId, ChainId};
use engine::DiLoop;
use project::project::Project;
use project::rig::RigProject;

use crate::command::Command;
use crate::di_loader::DiLoopSource;
use crate::dispatcher::{CommandDispatcher, EventStream};
use crate::event::Event;
use crate::selection_state::SelectionState;

/// In-process dispatcher backed by a shared `Project`.
///
/// Uses `Rc<RefCell<_>>` for interior mutability on the main (UI) thread.
/// This is NOT `Send` — see the note in `dispatcher.rs` about deferred
/// `Send + Sync` bounds.
pub struct LocalDispatcher {
    pub(crate) project: Rc<RefCell<Project>>,
    /// #436: the rig (presets/scenes) used to live only in the GUI and
    /// be mutated by hand in a wiring closure. It now lives behind the
    /// dispatcher so MIDI/MCP/GUI all go through `Command::ApplyRigNav`.
    /// `None` for non-rig sessions (legacy projects) — set via
    /// [`Self::attach_rig`] at project load.
    pub(crate) rig: RefCell<Option<Rc<RefCell<RigProject>>>>,
    /// #436: block selection used to be GUI-only state, so MIDI/MCP
    /// could not "click a block". It now lives here, set by
    /// `Command::SelectChainBlock`. Keyed by `ChainId` (works for
    /// `rig:<input>` and real ids); absent ⇒ nothing selected.
    pub(crate) selection: RefCell<HashMap<ChainId, usize>>,
    /// #555: filesystem directory where preset YAMLs live. Used by
    /// `Command::SaveChainPreset` / `DeleteChainPreset` so the
    /// dispatcher (not the GUI) owns the `fs::write` / `fs::remove_file`
    /// calls. `None` until the session attaches one via
    /// [`Self::attach_presets_path`]; preset I/O Commands error out
    /// cleanly until that happens.
    pub(crate) presets_path: RefCell<Option<PathBuf>>,
    /// #555: target path for `Command::SaveProject`. The dispatcher
    /// writes the `.openrig` (+ legacy `.yaml` sibling when the user-
    /// facing path is `.yaml`) itself instead of relying on the GUI to
    /// do `fs::write`. `None` until the session attaches one — preset
    /// dispatcher tests that don't exercise project save keep working
    /// unchanged.
    pub(crate) project_path: RefCell<Option<PathBuf>>,
    /// #555: target path for the project's sidecar `config.yaml`. The
    /// GUI used to compute this from `project_path.parent()` on save;
    /// the dispatcher now owns the resolution. `None` ⇒ derive from
    /// `project_path.parent().join("config.yaml")` at save time.
    pub(crate) config_path: RefCell<Option<PathBuf>>,
    /// #548: which chain / block the user has active on the Chains
    /// screen, plus snapshots of the toggle states. MIDI slots and the
    /// GUI both mutate this through `Command`s; `QueryKind::Selection`
    /// exposes it to MCP / gRPC. `Arc<RwLock<…>>` because the MIDI
    /// daemon thread reads it cross-thread.
    pub(crate) selection_state: Arc<RwLock<SelectionState>>,

    /// #614: ephemeral per-chain DI loop state — NEVER serialized into the
    /// project (persisting a DI source is a project-level concern tracked
    /// separately in #324). Each entry holds the original source enum and
    /// the decoded `Arc<DiLoop>` ready for lock-free audio-thread reads.
    /// The adapter-gui wiring (Task 6) calls `di_loop_for_chain` to
    /// retrieve the arc when `Event::ChainDiLoopEnabledChanged` fires.
    pub(crate) di_loop_state: RefCell<HashMap<ChainId, (DiLoopSource, Arc<DiLoop>)>>,

    /// #614: sample rate used for DI loop decoding + resampling.
    /// Defaults to 48 000 Hz; the adapter sets the real value via
    /// `attach_engine_sr` once the audio stream is running.
    pub(crate) engine_sr: RefCell<u32>,

    /// #693: completion channel for command work running on its own
    /// task (DI decode, catalog rescan, ...). Handlers spawn a task
    /// with a clone of the sender; `poll_async_results` (frontend
    /// tick) drains the receiver, applies state and emits the events.
    pub(crate) async_done_tx: std::sync::mpsc::Sender<AsyncDone>,
    pub(crate) async_done_rx: std::sync::mpsc::Receiver<AsyncDone>,
}

/// Completed off-thread command work (#693).
pub(crate) enum AsyncDone {
    /// DI-loop decode: install into `di_loop_state` + emit the event.
    DiLoad(ChainId, DiLoopSource, Result<Arc<DiLoop>, String>),
    /// Work whose state lives elsewhere (e.g. the global plugin
    /// registry): just surface the completion events.
    Events(Vec<Event>),
}

impl LocalDispatcher {
    /// Create a dispatcher that operates on the given shared `Project` handle.
    ///
    /// The caller (e.g. `adapter-gui`'s `ProjectSession`) should `Rc::clone`
    /// its own project handle and pass it here so both sides share the same
    /// allocation.
    pub fn new(project: Rc<RefCell<Project>>) -> Self {
        let (async_done_tx, async_done_rx) = std::sync::mpsc::channel();
        Self {
            project,
            rig: RefCell::new(None),
            selection: RefCell::new(HashMap::new()),
            presets_path: RefCell::new(None),
            project_path: RefCell::new(None),
            config_path: RefCell::new(None),
            selection_state: Arc::new(RwLock::new(SelectionState::default())),
            di_loop_state: RefCell::new(HashMap::new()),
            engine_sr: RefCell::new(48_000),
            async_done_tx,
            async_done_rx,
        }
    }

    /// The block index currently selected on `chain` (dispatcher-owned;
    /// the GUI renders this, MIDI/MCP can set it). `None` if unset.
    pub fn selected_block(&self, chain: &ChainId) -> Option<usize> {
        self.selection.borrow().get(chain).copied()
    }

    /// Shared handle to the GUI selection state. `Arc<RwLock<…>>` so
    /// the MIDI daemon thread can read the same state the GUI thread
    /// mutates; `Rc<RefCell<…>>` was tried first but `RefCell` is
    /// single-threaded and the daemon runs on its own midir-callback
    /// thread.
    pub fn selection_state(&self) -> Arc<RwLock<SelectionState>> {
        Arc::clone(&self.selection_state)
    }

    /// Share the session's `RigProject` handle so rig-nav commands can
    /// mutate the same allocation the GUI renders from. Idempotent.
    pub fn attach_rig(&self, rig: Rc<RefCell<RigProject>>) {
        *self.rig.borrow_mut() = Some(rig);
    }

    /// #693: clone the current state into an immutable snapshot for
    /// API-style reads (`crate::snapshot`). Called by
    /// `PublishingDispatcher` after every dispatch — the cost is one
    /// deep clone per command, paid on the writer thread, so readers
    /// never borrow the live `Rc` state.
    pub fn publish_state_snapshot(&self) {
        let project = self.project.borrow().clone();
        let rig = self.rig.borrow().as_ref().map(|rig| rig.borrow().clone());
        crate::snapshot::publish(crate::snapshot::StateSnapshot { project, rig });
    }

    /// #555: configure the preset library directory. Called by the
    /// session bootstrap once the resolved `presets_path` is known.
    /// Idempotent — calling this again replaces the path.
    pub fn attach_presets_path(&self, path: PathBuf) {
        *self.presets_path.borrow_mut() = Some(path);
    }

    /// #555: configure where `Command::SaveProject` writes the project
    /// file. Called by the session bootstrap and again on every "Save
    /// As" so the dispatcher and the GUI agree on the current target.
    pub fn attach_project_path(&self, path: PathBuf) {
        *self.project_path.borrow_mut() = Some(path);
    }

    /// #555: optional override for the sidecar `config.yaml` path.
    /// `None` ⇒ the dispatcher derives it from `project_path.parent()
    /// .join("config.yaml")` at save time (matches the pre-#555
    /// behaviour). Idempotent.
    pub fn attach_config_path(&self, path: Option<PathBuf>) {
        *self.config_path.borrow_mut() = path;
    }

    /// #614/#669: inform the dispatcher of the engine sample rate so DI loop
    /// decoding resamples to the correct target. Call this once the audio
    /// stream is running and whenever the device rate changes. Defaults to
    /// 48 000 Hz.
    ///
    /// #669: when the rate actually changes, every already-loaded DI loop is
    /// re-resampled to the new rate in place — a stale 48 kHz buffer plays in
    /// slow motion on a 44.1 kHz stream. Returns the chains whose loop arc was
    /// rebuilt so the caller can re-apply the fresh arc to any armed runtime.
    /// No-op (empty result) when the rate is unchanged.
    pub fn attach_engine_sr(&self, sr: u32) -> Vec<ChainId> {
        if *self.engine_sr.borrow() == sr {
            return Vec::new();
        }
        *self.engine_sr.borrow_mut() = sr;
        let mut rebuilt = Vec::new();
        let mut state = self.di_loop_state.borrow_mut();
        for (chain, (source, arc)) in state.iter_mut() {
            match crate::di_loader::load_di_loop(source, sr) {
                Ok(new_arc) => {
                    *arc = new_arc;
                    rebuilt.push(chain.clone());
                }
                Err(e) => {
                    // Off-thread; never silently swallow — a loop that fails to
                    // rebuild keeps its old (wrong-rate) buffer, so surface it.
                    eprintln!("[di-loop #669] rebuild for {chain:?} at {sr} Hz failed: {e}");
                }
            }
        }
        rebuilt
    }

    /// The sample rate the live engine is currently running at, as last
    /// synced via [`Self::attach_engine_sr`]. Authoritative fallback for any
    /// consumer that would otherwise assume a fixed rate (issue #723).
    pub fn engine_sr(&self) -> u32 {
        *self.engine_sr.borrow()
    }

    /// #614: retrieve the pre-loaded DI loop arc for `chain`, if any.
    ///
    /// The adapter-gui wiring (Task 6) calls this from the
    /// `ChainDiLoopEnabledChanged { enabled: true }` event handler to
    /// forward the arc to the chain's audio runtime. Returns `None` when
    /// no source has been loaded for this chain yet.
    pub fn di_loop_for_chain(&self, chain: &ChainId) -> Option<Arc<DiLoop>> {
        self.di_loop_state
            .borrow()
            .get(chain)
            .map(|(_, arc)| Arc::clone(arc))
    }

    /// #661: retrieve WHICH source is currently loaded for `chain`, if any.
    ///
    /// Parity twin of [`Self::di_loop_for_chain`]: the GUI reads this back so
    /// the DI loop popup's ComboBox can highlight the active source when it is
    /// reopened (the popup is re-instantiated on each show, so the selection
    /// must be re-derived from dispatcher state rather than held in the view).
    /// Returns `None` when no source has been loaded for this chain yet.
    pub fn di_loop_source_for_chain(&self, chain: &ChainId) -> Option<DiLoopSource> {
        self.di_loop_state
            .borrow()
            .get(chain)
            .map(|(source, _)| source.clone())
    }
}

impl CommandDispatcher for LocalDispatcher {
    fn dispatch(&self, cmd: Command) -> Result<Vec<Event>> {
        // Pure grouping switch: no logic, just routes each command to the
        // handler that owns its category. Behaviour is byte-identical to the
        // original flat match — each handler runs the original arm body
        // unchanged.
        match cmd {
            Command::SetBlockParameterNumber { .. }
            | Command::SetBlockParameterBool { .. }
            | Command::SetBlockParameterText { .. }
            | Command::SelectBlockParameterOption { .. }
            | Command::PickBlockParameterFile { .. } => self.handle_block_param(cmd),

            Command::ToggleBlockEnabled { .. }
            | Command::ReplaceBlockModel { .. }
            | Command::AddBlock { .. }
            | Command::InsertPrebuiltBlock { .. } => self.handle_block_lifecycle(cmd),

            Command::OverwriteBlock { .. }
            | Command::RemoveBlock { .. }
            | Command::MoveBlock { .. }
            | Command::SaveInsertBlock { .. } => self.handle_block_edit(cmd),

            Command::AddChain { .. }
            | Command::ConfigureChain { .. }
            | Command::RemoveChain { .. }
            | Command::SetChainVolume { .. } => self.handle_chain_crud(cmd),

            Command::MoveChainUp { .. }
            | Command::MoveChainDown { .. }
            | Command::ToggleChainEnabled { .. } => self.handle_chain_order(cmd),

            Command::SaveChain { .. }
            | Command::SaveChainInputEndpoints { .. }
            | Command::SaveChainOutputEndpoints { .. } => self.handle_chain_save(cmd),

            Command::SaveChainIo { .. } | Command::LoadChainPreset { .. } => {
                self.handle_chain_io_replace(cmd)
            }

            Command::SaveProject
            | Command::LoadProject { .. }
            | Command::CreateProject { .. }
            | Command::UpdateProjectName { .. }
            | Command::SaveAudioSettings { .. } => self.handle_project(cmd),

            // #513 / #493: system-side MIDI commands — no project mutation.
            // The adapter persists `config.yaml` / forwards to the daemon on
            // each event; the dispatcher just records the intent.
            Command::SaveMidiDevices { .. }
            | Command::StartMidiLearn
            | Command::StopMidiLearn
            | Command::PublishMidiEvent { .. } => self.handle_midi_system(cmd),

            // #513 / #493: project-side MIDI mapping — writes `project.midi`.
            Command::SaveMidiMapping { .. } => self.handle_project(cmd),

            Command::ApplyRigNav { .. } => self.handle_rig_nav(cmd),

            // #576: offline render — does not mutate the live project,
            // lives on the Command bus purely for transport-adapter
            // parity (MCP/gRPC auto-derive the tool via command_schema).
            Command::RenderChain {
                chain_path,
                input_path,
                output_path,
                start_s,
                end_s,
                sample_rate_hz,
                block_size,
                bit_depth,
                tail_ms,
            } => {
                // #693: bad args / missing input still error immediately
                // (cheap checks); only the render itself is deferred.
                crate::render_handler::precheck(bit_depth, &input_path)?;
                // #693: the offline render (file reads + full engine pass +
                // WAV write) runs on its own task. Completion — success or
                // failure — surfaces via poll_async_results as
                // RenderCompleted / Event::Error.
                let tx = self.async_done_tx.clone();
                std::thread::Builder::new()
                    .name("render-chain".into())
                    .spawn(move || {
                        let done = match crate::render_handler::run(
                            chain_path,
                            input_path,
                            output_path,
                            start_s,
                            end_s,
                            sample_rate_hz,
                            block_size,
                            bit_depth,
                            tail_ms,
                        ) {
                            Ok(ev) => ev,
                            Err(e) => Event::Error {
                                message: format!("RenderChain failed: {e}"),
                            },
                        };
                        let _ = tx.send(AsyncDone::Events(vec![done]));
                    })
                    .map_err(|e| anyhow::anyhow!("failed to spawn render-chain task: {e}"))?;
                Ok(vec![])
            }

            Command::CaptureRigEdits => self.handle_capture_rig_edits(),

            Command::RenameRigPreset { .. } => self.handle_rename_rig_preset(cmd),

            Command::SelectChainBlock { chain, block_index } => {
                // Legacy per-chain selection map (kept for the old GUI
                // wiring that hasn't migrated yet).
                self.selection
                    .borrow_mut()
                    .insert(chain.clone(), block_index);
                // #548: mirror the click into the GUI selection state
                // the MIDI daemon reads. Resolve the block id from the
                // index inside the project — slots address blocks by id.
                {
                    let project = self.project.borrow();
                    let block_id = project
                        .chains
                        .iter()
                        .find(|c| c.id == chain)
                        .and_then(|c| c.blocks.get(block_index))
                        .map(|b| b.id.0.clone());
                    if let Ok(mut sel) = self.selection_state.write() {
                        sel.active_chain = Some(chain.0.clone());
                        sel.active_block = block_id;
                    }
                }
                Ok(vec![Event::ProjectMutated])
            }

            Command::SelectActiveChain { chain } => self.handle_select_active_chain(chain),

            Command::SetLanguage { .. } => self.handle_set_language(cmd),

            Command::SetOutputMuted { .. } => self.handle_set_output_muted(cmd),

            Command::RemoveRecentProject { .. } => self.handle_remove_recent_project(cmd),

            Command::SaveChainPreset { .. } | Command::DeleteChainPreset { .. } => {
                self.handle_chain_preset(cmd)
            }

            Command::SetTunerEnabled { .. } | Command::SetSpectrumEnabled { .. } => {
                self.handle_diagnostic_enabled(cmd)
            }

            Command::CloseProject => self.handle_close_project(cmd),

            Command::RegisterRecentProject { .. } | Command::MarkRecentProjectInvalid { .. } => {
                self.handle_recent_register(cmd)
            }

            // #513: system-level paths overrides. No project mutation —
            // the adapter persists `config.yaml` on `Event::PathsSaved`,
            // mirroring `SaveMidiDevices` (ADR 0003).
            Command::SetPresetsPath { .. }
            | Command::SetPluginsPath { .. }
            | Command::SetEvaluationsPath { .. } => self.handle_paths_system(cmd),

            // #561: hot-reload the plugin catalog (no payload).
            Command::ReloadPluginCatalog => self.handle_reload_plugin_catalog(),
            // #561 (expanded scope): per-plugin load / unload.
            Command::LoadPlugin { id } => self.handle_load_plugin(id),
            Command::UnloadPlugin { id } => self.handle_unload_plugin(id),

            // #548: selection / view mutations driven by MIDI slots.
            Command::SelectActiveChainRelative { delta } => {
                self.handle_select_active_chain_relative(delta)
            }
            Command::SelectActiveBlockRelative { delta } => {
                self.handle_select_active_block_relative(delta)
            }
            Command::SetCompactViewEnabled { enabled } => {
                self.selection_state
                    .write()
                    .expect("selection state poisoned")
                    .compact_view_enabled = enabled;
                // #591: emit so the adapter can open/close the compact view
                // for the active chain — the MIDI footswitch path drains
                // events and had nothing to act on before.
                Ok(vec![Event::CompactViewEnabledChanged { enabled }])
            }
            Command::ToggleActiveBlockNeighborEnabled => {
                self.handle_toggle_active_block_neighbor_enabled()
            }

            // #712: per-machine MIDI/MCP master switches → config.yaml.
            Command::SetMidiEnabled { enabled } => self.handle_set_midi_enabled(enabled),
            Command::SetMcpEnabled { enabled } => self.handle_set_mcp_enabled(enabled),

            // #614: per-chain virtual DI loop (ephemeral, never persisted).
            Command::SetChainDiLoopSource { .. } | Command::SetChainDiLoopEnabled { .. } => {
                self.handle_di_loop(cmd)
            }

            // #716: per-machine I/O binding registry (persisted to config.yaml).
            Command::CreateIoBinding { binding } | Command::UpdateIoBinding { binding } => {
                self.handle_create_or_update_io_binding(binding)
            }
            Command::DeleteIoBinding { id } => self.handle_delete_io_binding(id),
            Command::RenameIoBinding { id, name } => self.handle_rename_io_binding(id, name),
            Command::AddIoEndpoint {
                binding_id,
                is_input,
                device_id,
                channels,
                mode,
            } => self.handle_add_io_endpoint(binding_id, is_input, device_id, channels, mode),
            Command::RemoveIoEndpoint {
                binding_id,
                is_input,
                endpoint_name,
            } => self.handle_remove_io_endpoint(binding_id, is_input, endpoint_name),
        }
    }

    fn subscribe(&self) -> EventStream {
        // Phase 2 will return a real event stream. For now this is a no-op.
    }

    /// #693: install completed off-thread DI decodes and emit their
    /// events. Failures are logged (non-blocking logger) — same policy
    /// as every other async side-effect.
    fn poll_async_results(&self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(done) = self.async_done_rx.try_recv() {
            match done {
                AsyncDone::DiLoad(chain, source, result) => match result {
                    Ok(arc) => {
                        self.di_loop_state
                            .borrow_mut()
                            .insert(chain.clone(), (source, arc));
                        events.push(Event::ChainDiLoopSourceChanged { chain });
                    }
                    Err(e) => log::error!("DI loop load failed for chain '{}': {e}", chain.0),
                },
                AsyncDone::Events(completed) => events.extend(completed),
            }
        }
        events
    }
}

impl LocalDispatcher {
    /// Borrow the project mutably, locate `chain` then `block`, and run `f`
    /// against the located block. Centralises the chain-not-found /
    /// block-not-found lookup that every block-scoped arm performed inline.
    ///
    /// `pub(crate)` so the per-feature `handle_*` modules
    /// (`local_dispatcher_block_*`, `local_dispatcher_chain_*`) can share it.
    ///
    /// Behaviour is byte-identical to the previous inline form: same
    /// `borrow_mut` scope, same `find` order, same error strings, same `?`
    /// propagation point.
    pub(crate) fn with_block<R>(
        &self,
        chain: &ChainId,
        block: &BlockId,
        f: impl FnOnce(&mut project::block::AudioBlock) -> Result<R>,
    ) -> Result<R> {
        let mut proj = self.project.borrow_mut();
        let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == *chain) else {
            return Err(anyhow::anyhow!("chain not found: {:?}", chain));
        };
        let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == *block) else {
            return Err(anyhow::anyhow!("block not found: {:?}", block));
        };
        f(target_block)
    }

    /// Borrow the project mutably, locate `chain`, and run `f` against it.
    /// Centralises the chain-not-found lookup shared by chain-scoped arms.
    ///
    /// `pub(crate)` so the per-feature `handle_*` modules can share it.
    ///
    /// Behaviour is byte-identical to the previous inline form: same
    /// `borrow_mut` scope, same `find` order, same error string.
    pub(crate) fn with_chain<R>(
        &self,
        chain: &ChainId,
        f: impl FnOnce(&mut project::chain::Chain) -> Result<R>,
    ) -> Result<R> {
        let mut proj = self.project.borrow_mut();
        let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == *chain) else {
            return Err(anyhow::anyhow!("chain not found: {:?}", chain));
        };
        f(target_chain)
    }
}

// ── Per-feature handlers (file-per-feature; #436 dispatcher split) ──────────
// This file is the thin router only. Each `handle_*` the `dispatch` match
// calls lives in its own sibling module (declared in `lib.rs`):
//   local_dispatcher_block_param     · handle_block_param
//   local_dispatcher_block_lifecycle · handle_block_lifecycle
//   local_dispatcher_block_edit      · handle_block_edit
//   local_dispatcher_chain_crud      · handle_chain_crud
//   local_dispatcher_chain_order     · handle_chain_order
//   local_dispatcher_chain_save      · handle_chain_save
//   local_dispatcher_chain_io        · handle_chain_io_replace
//   local_dispatcher_project         · handle_project
//   local_dispatcher_rig             · handle_rig_nav / capture / rename
// Each adds an `impl LocalDispatcher` block; behaviour is byte-identical to
// the previous single-file form (arm bodies moved verbatim).

impl LocalDispatcher {
    /// #513 / #493: system-side MIDI commands. None of these touch the
    /// project (MIDI device selection is per-machine / ADR 0003; learn-mode
    /// is daemon state; PublishMidiEvent is a passthrough of a raw event the
    /// daemon submits through the existing command bridge so the publishing
    /// dispatcher's fan-out remains the single transport). Each arm only
    /// records the intent via an `Event` — the adapter does the actual work
    /// (persist config.yaml, toggle learn-mode flag, route the event into
    /// the mapping editor).
    pub(crate) fn handle_midi_system(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SaveMidiDevices { .. } => Ok(vec![Event::MidiDevicesSaved]),
            Command::StartMidiLearn => Ok(vec![Event::MidiLearnStarted]),
            Command::StopMidiLearn => Ok(vec![Event::MidiLearnStopped]),
            Command::PublishMidiEvent { source } => Ok(vec![Event::MidiEventReceived { source }]),
            other => {
                unreachable!("handle_midi_system received non-midi-system command: {other:?}")
            }
        }
    }

    /// #513 / #540: system-level paths overrides (presets, plugins).
    /// The command owns the persistence: write the picked path into
    /// `config.yaml` (ADR 0003 — system setting), then emit
    /// `Event::PathsSaved` so listeners (GUI label refresh, MCP, gRPC)
    /// can pick up the change without re-reading from disk.
    ///
    /// The previous handler (#513) emitted the event only and relied on
    /// "the adapter persists on PathsSaved" — but the event carries no
    /// path payload and no listener was wired, so the user's pick
    /// survived only in memory and reset to default on the next launch
    /// (issue #540).
    pub(crate) fn handle_paths_system(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            // #693: path persistence runs on the persist worker — the
            // dispatching thread never waits on disk; errors go to the
            // non-blocking logger.
            // #731: bind the config path at dispatch time (see app_config_persist);
            // the worker must not re-resolve `$HOME` at write time.
            Command::SetPresetsPath { path } => {
                crate::app_config_persist::persist_app_config(move |config| {
                    config.paths.presets_path = path;
                });
                Ok(vec![Event::PathsSaved])
            }
            Command::SetPluginsPath { path } => {
                crate::app_config_persist::persist_app_config(move |config| {
                    config.paths.plugins_path = path;
                });
                Ok(vec![Event::PathsSaved])
            }
            Command::SetEvaluationsPath { path } => {
                crate::app_config_persist::persist_app_config(move |config| {
                    config.paths.evaluations_path = path;
                });
                Ok(vec![Event::PathsSaved])
            }
            other => {
                unreachable!("handle_paths_system received non-paths command: {other:?}")
            }
        }
    }
}
