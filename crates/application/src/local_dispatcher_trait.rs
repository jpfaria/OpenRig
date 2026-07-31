//! The single `impl CommandDispatcher for LocalDispatcher` block (#127).
//!
//! Rust allows exactly one trait impl per (trait, type) pair in a crate
//! (E0119 otherwise), so every `CommandDispatcher` method for
//! `LocalDispatcher` — `dispatch`, `poll_async_results`, and the 11
//! GUI-facing methods added by #127 — has to live in this one block. To
//! keep it thin, the 11 GUI-facing methods are one-line delegations to
//! inherent `pub(crate)` methods whose bodies stay exactly where they
//! already lived (`local_dispatcher_queries.rs`, `local_dispatcher_attach.rs`)
//! — zero logic moved, only where the trait-shaped entry point lives.
//! `dispatch`/`poll_async_results` moved here verbatim from
//! `local_dispatcher.rs`, which was already at its line cap.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use anyhow::Result;

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use engine::DiPcm;
use project::rig::RigProject;

use crate::command::{
    BlockCommand, ChainCommand, Command, IoBindingCommand, MidiCommand, PluginCommand,
    ProjectCommand, SelectionCommand, SettingsCommand,
};
use crate::di_loader::DiLoopSource;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::{AsyncDone, LocalDispatcher, ToneDoctorInput};
use crate::metronome_state::{MetronomeControlState, MetronomeSnapshot};
use crate::runtime_control::RuntimeControl;
use crate::selection_state::SelectionState;
use crate::tone_doctor_report::ToneRun;

impl CommandDispatcher for LocalDispatcher {
    fn dispatch(&self, cmd: Command) -> Result<Vec<Event>> {
        // Pure grouping switch: no logic, just routes each command to the
        // handler that owns its category. Behaviour is byte-identical to the
        // original flat match — each handler runs the original arm body
        // unchanged.
        match cmd {
            Command::Block(
                BlockCommand::SetBlockParameterNumber { .. }
                | BlockCommand::SetBlockParameterBool { .. }
                | BlockCommand::SetBlockParameterText { .. }
                | BlockCommand::SelectBlockParameterOption { .. }
                | BlockCommand::PickBlockParameterFile { .. },
            ) => self.handle_block_param(cmd),

            Command::Block(
                BlockCommand::ToggleBlockEnabled { .. }
                | BlockCommand::ReplaceBlockModel { .. }
                | BlockCommand::AddBlock { .. }
                | BlockCommand::InsertPrebuiltBlock { .. },
            ) => self.handle_block_lifecycle(cmd),

            Command::Settings(SettingsCommand::RefreshAudioDevices) => {
                self.handle_refresh_audio_devices()
            }

            Command::Block(
                BlockCommand::OverwriteBlock { .. }
                | BlockCommand::RemoveBlock { .. }
                | BlockCommand::MoveBlock { .. }
                | BlockCommand::SaveInsertBlock { .. },
            ) => self.handle_block_edit(cmd),

            Command::Chain(
                ChainCommand::AddChain { .. }
                | ChainCommand::ConfigureChain { .. }
                | ChainCommand::RemoveChain { .. }
                | ChainCommand::SetChainVolume { .. }
                | ChainCommand::SetChainIoBindings { .. },
            ) => self.handle_chain_crud(cmd),

            Command::Chain(
                ChainCommand::MoveChainUp { .. }
                | ChainCommand::MoveChainDown { .. }
                | ChainCommand::ToggleChainEnabled { .. },
            ) => self.handle_chain_order(cmd),

            // #127: runtime control, not a project mutation — the effect is
            // applied through the attached `RuntimeControl`.
            Command::Chain(ChainCommand::SyncChainRuntime { chain }) => {
                self.handle_sync_chain_runtime(chain)
            }

            Command::Chain(
                ChainCommand::SaveChain { .. }
                | ChainCommand::SaveChainInputEndpoints { .. }
                | ChainCommand::SaveChainOutputEndpoints { .. },
            ) => self.handle_chain_save(cmd),

            Command::Chain(
                ChainCommand::SaveChainIo { .. } | ChainCommand::LoadChainPreset { .. },
            ) => self.handle_chain_io_replace(cmd),

            Command::Project(
                ProjectCommand::SaveProject
                | ProjectCommand::LoadProject { .. }
                | ProjectCommand::CreateProject { .. }
                | ProjectCommand::UpdateProjectName { .. },
            )
            | Command::Settings(SettingsCommand::SaveAudioSettings { .. }) => {
                self.handle_project(cmd)
            }

            // #513 / #493: system-side MIDI commands — no project mutation.
            // The adapter persists `config.yaml` / forwards to the daemon on
            // each event; the dispatcher just records the intent.
            Command::Midi(
                MidiCommand::SaveMidiDevices { .. }
                | MidiCommand::StartMidiLearn
                | MidiCommand::StopMidiLearn
                | MidiCommand::PublishMidiEvent { .. },
            ) => self.handle_midi_system(cmd),

            // #513 / #493: project-side MIDI mapping — writes `project.midi`.
            Command::Midi(MidiCommand::SaveMidiMapping { .. }) => self.handle_project(cmd),

            Command::Selection(SelectionCommand::ApplyRigNav { .. }) => self.handle_rig_nav(cmd),

            // #576: offline render — does not mutate the live project,
            // lives on the Command bus purely for transport-adapter
            // parity (MCP/gRPC auto-derive the tool via command_schema).
            Command::Chain(ChainCommand::RenderChain {
                chain_path,
                input_path,
                output_path,
                start_s,
                end_s,
                sample_rate_hz,
                block_size,
                bit_depth,
                tail_ms,
            }) => {
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

            Command::Project(ProjectCommand::CaptureRigEdits) => self.handle_capture_rig_edits(),

            Command::Selection(SelectionCommand::RenameRigPreset { .. }) => {
                self.handle_rename_rig_preset(cmd)
            }

            Command::Selection(SelectionCommand::SelectChainBlock { chain, block_index }) => {
                // #548: record the click in the GUI selection state that
                // MIDI/MCP/gRPC read (`QueryKind::Selection`). Resolve the
                // block id from the index inside the project — slots address
                // blocks by id.
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

            Command::Selection(SelectionCommand::SelectActiveChain { chain }) => {
                self.handle_select_active_chain(chain)
            }

            Command::Settings(SettingsCommand::SetLanguage { .. }) => self.handle_set_language(cmd),

            Command::Selection(SelectionCommand::SetOutputMuted { .. }) => {
                self.handle_set_output_muted(cmd)
            }

            Command::Project(ProjectCommand::RemoveRecentProject { .. }) => {
                self.handle_remove_recent_project(cmd)
            }

            Command::Chain(
                ChainCommand::SaveChainPreset { .. } | ChainCommand::DeleteChainPreset { .. },
            ) => self.handle_chain_preset(cmd),

            Command::Selection(
                SelectionCommand::SetTunerEnabled { .. }
                | SelectionCommand::SetSpectrumEnabled { .. },
            ) => self.handle_diagnostic_enabled(cmd),

            Command::Metronome(_) => self.handle_metronome(cmd),

            // #791: the Tone Doctor — diagnosis and its measured fix, on the
            // bus so MCP/gRPC reach the same verdict the GUI panel shows.
            Command::ToneDoctor(_) => self.handle_tone_doctor(cmd),

            // #127: both stop the rig. `CloseProject` also ends the session;
            // `StopProjectRuntime` leaves it open (what opening ANOTHER project
            // does to the one already running).
            Command::Project(ProjectCommand::CloseProject | ProjectCommand::StopProjectRuntime) => {
                self.handle_close_project(cmd)
            }

            Command::Project(
                ProjectCommand::RegisterRecentProject { .. }
                | ProjectCommand::MarkRecentProjectInvalid { .. },
            ) => self.handle_recent_register(cmd),

            // #513: system-level paths overrides. No project mutation —
            // the adapter persists `config.yaml` on `Event::PathsSaved`,
            // mirroring `SaveMidiDevices` (ADR 0003).
            Command::Settings(
                SettingsCommand::SetPresetsPath { .. }
                | SettingsCommand::SetPluginsPath { .. }
                | SettingsCommand::SetEvaluationsPath { .. },
            ) => self.handle_paths_system(cmd),

            // #561: hot-reload the plugin catalog (no payload).
            Command::Plugin(PluginCommand::ReloadPluginCatalog) => {
                self.handle_reload_plugin_catalog()
            }
            // #561 (expanded scope): per-plugin load / unload.
            Command::Plugin(PluginCommand::LoadPlugin { id }) => self.handle_load_plugin(id),
            Command::Plugin(PluginCommand::UnloadPlugin { id }) => self.handle_unload_plugin(id),

            // #548: selection / view mutations driven by MIDI slots.
            Command::Selection(SelectionCommand::SelectActiveChainRelative { delta }) => {
                self.handle_select_active_chain_relative(delta)
            }
            Command::Selection(SelectionCommand::SelectActiveBlockRelative { delta }) => {
                self.handle_select_active_block_relative(delta)
            }
            Command::Selection(SelectionCommand::SetCompactViewEnabled { enabled }) => {
                self.selection_state
                    .write()
                    .expect("selection state poisoned")
                    .compact_view_enabled = enabled;
                // #591: emit so the adapter can open/close the compact view
                // for the active chain — the MIDI footswitch path drains
                // events and had nothing to act on before.
                Ok(vec![Event::CompactViewEnabledChanged { enabled }])
            }
            Command::Selection(SelectionCommand::ToggleActiveBlockNeighborEnabled) => {
                self.handle_toggle_active_block_neighbor_enabled()
            }

            // #712: per-machine MIDI/MCP master switches → config.yaml.
            Command::Midi(MidiCommand::SetMidiEnabled { enabled }) => {
                self.handle_set_midi_enabled(enabled)
            }
            Command::Settings(SettingsCommand::SetMcpEnabled { enabled }) => {
                self.handle_set_mcp_enabled(enabled)
            }

            // #614/#717: per-chain virtual DI loop (source/enabled ephemeral;
            // output persisted into project via SetChainDiLoopOutput).
            Command::Chain(
                ChainCommand::SetChainDiLoopSource { .. }
                | ChainCommand::SetChainDiLoopEnabled { .. }
                | ChainCommand::SetChainDiLoopOutput { .. },
            ) => self.handle_di_loop(cmd),

            // #323: per-chain loopers (membership + params persisted; the
            // transport is runtime state and travels as an event).
            Command::Looper(_) => self.handle_looper(cmd),

            // #716: per-machine I/O binding registry (persisted to config.yaml).
            Command::IoBinding(
                IoBindingCommand::CreateIoBinding { binding }
                | IoBindingCommand::UpdateIoBinding { binding },
            ) => self.handle_create_or_update_io_binding(binding),
            Command::IoBinding(IoBindingCommand::DeleteIoBinding { id }) => {
                self.handle_delete_io_binding(id)
            }
            Command::IoBinding(IoBindingCommand::RenameIoBinding { id, name }) => {
                self.handle_rename_io_binding(id, name)
            }
            Command::IoBinding(IoBindingCommand::AddIoEndpoint {
                binding_id,
                is_input,
                device_id,
                channels,
                mode,
            }) => self.handle_add_io_endpoint(binding_id, is_input, device_id, channels, mode),
            Command::IoBinding(IoBindingCommand::RemoveIoEndpoint {
                binding_id,
                is_input,
                endpoint_name,
            }) => self.handle_remove_io_endpoint(binding_id, is_input, endpoint_name),

            // #127: push the effective registry into the LIVE runtime (not a
            // persist — the CRUD arms above own `config.yaml`).
            Command::IoBinding(IoBindingCommand::SetIoBindings) => self.handle_set_io_bindings(),
        }
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
                AsyncDone::ToneDiagnosis(chain, result) => match result {
                    Ok(report) => {
                        self.tone_doctor_runs
                            .borrow_mut()
                            .insert(chain.clone(), ToneRun::finished(report.clone()));
                        events.push(Event::ChainToneDiagnosed { chain, report });
                    }
                    Err(e) => {
                        // A reader-only transport never sees this event, so the
                        // failure is recorded where it can read it back.
                        self.tone_doctor_runs
                            .borrow_mut()
                            .insert(chain.clone(), ToneRun::failed(e.clone()));
                        events.push(Event::Error {
                            message: format!("tone diagnosis failed for chain '{}': {e}", chain.0),
                        });
                    }
                },
                AsyncDone::Events(completed) => events.extend(completed),
            }
        }
        events
    }

    // ── #127: GUI-facing surface — one-line delegations to the inherent
    // ── methods whose bodies live in `local_dispatcher_queries.rs` /
    // ── `local_dispatcher_attach.rs`. Zero logic here, on purpose.

    fn selection_state(&self) -> Arc<RwLock<SelectionState>> {
        LocalDispatcher::selection_state(self)
    }

    fn engine_sr(&self) -> u32 {
        LocalDispatcher::engine_sr(self)
    }

    fn chain_snapshot(&self, chain: &ChainId) -> Option<project::chain::Chain> {
        LocalDispatcher::chain_snapshot(self, chain)
    }

    fn di_loop_for_chain(&self, chain: &ChainId) -> Option<Arc<DiPcm>> {
        LocalDispatcher::di_loop_for_chain(self, chain)
    }

    fn di_loop_source_for_chain(&self, chain: &ChainId) -> Option<DiLoopSource> {
        LocalDispatcher::di_loop_source_for_chain(self, chain)
    }

    fn tone_report_json(&self, chain: &ChainId) -> String {
        LocalDispatcher::tone_report_json(self, chain)
    }

    fn attach_rig(&self, rig: Rc<RefCell<RigProject>>) {
        LocalDispatcher::attach_rig(self, rig)
    }

    fn attach_presets_path(&self, path: PathBuf) {
        LocalDispatcher::attach_presets_path(self, path)
    }

    fn attach_project_path(&self, path: PathBuf) {
        LocalDispatcher::attach_project_path(self, path)
    }

    fn attach_config_path(&self, path: Option<PathBuf>) {
        LocalDispatcher::attach_config_path(self, path)
    }

    fn attach_engine_sr(&self, sr: u32) -> Vec<ChainId> {
        LocalDispatcher::attach_engine_sr(self, sr)
    }

    fn attach_tone_doctor_input(&self, provider: ToneDoctorInput) {
        LocalDispatcher::attach_tone_doctor_input(self, provider)
    }

    fn attach_runtime_control(&self, control: Rc<dyn RuntimeControl>) {
        LocalDispatcher::attach_runtime_control(self, control)
    }

    fn attach_io_bindings(&self, registry: Rc<RefCell<Vec<IoBinding>>>) {
        LocalDispatcher::attach_io_bindings(self, registry)
    }

    fn attach_metronome_state(&self, state: Rc<RefCell<MetronomeControlState>>) {
        LocalDispatcher::attach_metronome_state(self, state)
    }

    fn metronome_snapshot(&self) -> MetronomeSnapshot {
        LocalDispatcher::metronome_snapshot(self)
    }
}
