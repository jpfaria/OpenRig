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
//! `dispatch` is a thin router: it groups commands by category and delegates
//! each to the `handle_*` method that owns it — one per sibling
//! `local_dispatcher_<feature>` module. The read accessors, the dependency-
//! attach setters, and the shared chain/block borrow helpers live in their own
//! sibling modules too (`local_dispatcher_queries` / `_attach` / `_access`),
//! keeping this file to the struct definition, construction, and the router
//! (issue #792 single-responsibility split).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use anyhow::Result;

use domain::ids::ChainId;
use engine::DiPcm;
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
    /// #792 / ADR-0003: the per-machine SYSTEM config path — where the I/O
    /// binding registry persists. SEPARATE from `config_path` (the project
    /// sidecar): opening a project sets `config_path` to `<project>/config.yaml`,
    /// and the per-machine registry must NOT follow it there. `None` ⇒
    /// `FilesystemStorage::app_config_path()`; tests attach a temp path.
    pub(crate) io_config_path: RefCell<Option<PathBuf>>,
    /// #548: which chain / block the user has active on the Chains
    /// screen, plus snapshots of the toggle states. MIDI slots and the
    /// GUI both mutate this through `Command`s; `QueryKind::Selection`
    /// exposes it to MCP / gRPC. `Arc<RwLock<…>>` because the MIDI
    /// daemon thread reads it cross-thread.
    pub(crate) selection_state: Arc<RwLock<SelectionState>>,

    /// #614: ephemeral per-chain DI loop state — NEVER serialized into the
    /// project (persisting a DI source is a project-level concern tracked
    /// separately in #324). Each entry holds the original source enum and
    /// the decoded `Arc<DiPcm>` (un-resampled source) ready for the arm path
    /// to resample per output-stream rate (#749). The adapter-gui wiring
    /// (Task 6) calls `di_loop_for_chain` to retrieve it when
    /// `Event::ChainDiLoopEnabledChanged` fires.
    pub(crate) di_loop_state: RefCell<HashMap<ChainId, (DiLoopSource, Arc<DiPcm>)>>,

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
    DiLoad(ChainId, DiLoopSource, Result<Arc<DiPcm>, String>),
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
            io_config_path: RefCell::new(None),
            selection_state: Arc::new(RwLock::new(SelectionState::default())),
            di_loop_state: RefCell::new(HashMap::new()),
            engine_sr: RefCell::new(48_000),
            async_done_tx,
            async_done_rx,
        }
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
            | Command::SetChainVolume { .. }
            | Command::SetChainIoBindings { .. } => self.handle_chain_crud(cmd),

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

            // #614/#717: per-chain virtual DI loop (source/enabled ephemeral;
            // output persisted into project via SetChainDiLoopOutput).
            Command::SetChainDiLoopSource { .. }
            | Command::SetChainDiLoopEnabled { .. }
            | Command::SetChainDiLoopOutput { .. } => self.handle_di_loop(cmd),

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
