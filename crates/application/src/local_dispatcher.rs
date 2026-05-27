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
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::Result;

use domain::ids::{BlockId, ChainId};
use project::project::Project;
use project::rig::RigProject;

use crate::command::Command;
use crate::dispatcher::{CommandDispatcher, EventStream};
use crate::event::Event;

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
    pub(crate) selection: RefCell<std::collections::HashMap<ChainId, usize>>,
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
}

impl LocalDispatcher {
    /// Create a dispatcher that operates on the given shared `Project` handle.
    ///
    /// The caller (e.g. `adapter-gui`'s `ProjectSession`) should `Rc::clone`
    /// its own project handle and pass it here so both sides share the same
    /// allocation.
    pub fn new(project: Rc<RefCell<Project>>) -> Self {
        Self {
            project,
            rig: RefCell::new(None),
            selection: RefCell::new(std::collections::HashMap::new()),
            presets_path: RefCell::new(None),
            project_path: RefCell::new(None),
            config_path: RefCell::new(None),
        }
    }

    /// The block index currently selected on `chain` (dispatcher-owned;
    /// the GUI renders this, MIDI/MCP can set it). `None` if unset.
    pub fn selected_block(&self, chain: &ChainId) -> Option<usize> {
        self.selection.borrow().get(chain).copied()
    }

    /// Share the session's `RigProject` handle so rig-nav commands can
    /// mutate the same allocation the GUI renders from. Idempotent.
    pub fn attach_rig(&self, rig: Rc<RefCell<RigProject>>) {
        *self.rig.borrow_mut() = Some(rig);
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

            Command::CaptureRigEdits => self.handle_capture_rig_edits(),

            Command::RenameRigPreset { .. } => self.handle_rename_rig_preset(cmd),

            Command::SelectChainBlock { chain, block_index } => {
                self.selection
                    .borrow_mut()
                    .insert(chain.clone(), block_index);
                Ok(vec![Event::ProjectMutated])
            }

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
            Command::SetPresetsPath { .. } | Command::SetPluginsPath { .. } => {
                self.handle_paths_system(cmd)
            }

            Command::SeparateStems { .. } => self.handle_separate_stems(cmd),

            Command::LoadTrack { .. }
            | Command::UnloadTrack
            | Command::RenameTrack { .. }
            | Command::DeleteTrack { .. } => self.handle_track_lifecycle(cmd),

            Command::TrackPlay | Command::TrackPause | Command::TrackSeek { .. } => {
                self.handle_track_transport(cmd)
            }

            Command::SetStemMute { .. }
            | Command::SetStemSolo { .. }
            | Command::SetStemGain { .. }
            | Command::SetStemPan { .. } => self.handle_stem_controls(cmd),
            // #561: hot-reload the plugin catalog (no payload).
            Command::ReloadPluginCatalog => self.handle_reload_plugin_catalog(),
            // #561 (expanded scope): per-plugin load / unload.
            Command::LoadPlugin { id } => self.handle_load_plugin(id),
            Command::UnloadPlugin { id } => self.handle_unload_plugin(id),
        }
    }

    fn subscribe(&self) -> EventStream {
        // Phase 2 will return a real event stream. For now this is a no-op.
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
        use infra_filesystem::FilesystemStorage;
        match cmd {
            Command::SetPresetsPath { path } => {
                FilesystemStorage::save_presets_path(path)
                    .map_err(|e| anyhow::anyhow!("save_presets_path failed: {e}"))?;
                Ok(vec![Event::PathsSaved])
            }
            Command::SetPluginsPath { path } => {
                FilesystemStorage::save_plugins_path(path)
                    .map_err(|e| anyhow::anyhow!("save_plugins_path failed: {e}"))?;
                Ok(vec![Event::PathsSaved])
            }
            other => {
                unreachable!("handle_paths_system received non-paths command: {other:?}")
            }
        }
    }
}
