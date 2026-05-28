//! Project lifecycle/settings handler.
//!
//! #555 round 2: `Command::SaveProject` now owns the actual file
//! writes (project YAML + sidecar config + sibling presets/ dir).
//! The GUI used to do them inline in
//! `adapter-gui::project_ops::save_project_session` before dispatching
//! the empty event — a violation of "tela sem regra de negócio". MCP /
//! gRPC clients dispatching `SaveProject` now get the same on-disk
//! effect as the GUI.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use infra_filesystem::{FilesystemStorage, GuiAudioDeviceSettings};

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::project_save::build_rig_for_save;

impl LocalDispatcher {
    /// Project lifecycle + settings commands.
    pub(crate) fn handle_project(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            // ── Project lifecycle ─────────────────────────────────────────────
            Command::SaveProject => {
                // The path is set by the GUI on session bootstrap and
                // re-attached on Save As via `attach_project_path`.
                // Older tests dispatch `SaveProject` without attaching
                // a path — preserve that no-op + event semantics so
                // they keep working unchanged; only do the file I/O
                // when the path is actually configured.
                if let Some(project_path) = self.project_path.borrow().clone() {
                    self.save_project_to_disk(&project_path)?;
                }
                Ok(vec![Event::ProjectSaved])
            }

            Command::LoadProject { project, path: _ } => {
                // Replace the shared project data in-place so all Rc::clone
                // holders (adapter-gui's ProjectSession) see the updated state.
                *self.project.borrow_mut() = project;
                Ok(vec![Event::ProjectLoaded, Event::ProjectMutated])
            }

            Command::CreateProject { project } => {
                *self.project.borrow_mut() = project;
                Ok(vec![Event::ProjectCreated, Event::ProjectMutated])
            }

            // ── Project settings ──────────────────────────────────────────────
            Command::UpdateProjectName { name } => {
                let trimmed = name.trim().to_string();
                self.project.borrow_mut().name = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                };
                Ok(vec![Event::ProjectMutated])
            }

            Command::SaveAudioSettings { device_settings } => {
                self.project.borrow_mut().device_settings = device_settings.clone();

                // #581: persist the pick into the per-machine
                // `config.yaml` so it survives restarts and so every
                // transport (GUI, MCP, gRPC) dispatching this command
                // gets the same durable effect — per the Command-bus
                // parity LAW. Touch only `input_devices` /
                // `output_devices`, preserving the rest of `AppConfig`
                // (language, midi_devices, paths, recent_projects).
                let gui_devices: Vec<GuiAudioDeviceSettings> = device_settings
                    .iter()
                    .map(|s| GuiAudioDeviceSettings {
                        device_id: s.device_id.0.clone(),
                        // `name` is for UI display; `DeviceSettings`
                        // does not carry it. The GUI rebuilds the
                        // human name from the live cpal descriptor
                        // (`build_project_device_rows`), so an empty
                        // string here is safe.
                        name: String::new(),
                        sample_rate: s.sample_rate,
                        buffer_size_frames: s.buffer_size_frames,
                        bit_depth: s.bit_depth,
                        #[cfg(target_os = "linux")]
                        realtime: s.realtime,
                        #[cfg(target_os = "linux")]
                        rt_priority: s.rt_priority,
                        #[cfg(target_os = "linux")]
                        nperiods: s.nperiods,
                    })
                    .collect();
                let mut config = FilesystemStorage::load_app_config().unwrap_or_default();
                config.input_devices = gui_devices.clone();
                config.output_devices = gui_devices;
                FilesystemStorage::save_app_config(&config)
                    .map_err(|e| anyhow!("failed to persist audio settings: {e}"))?;

                Ok(vec![Event::AudioSettingsSaved])
            }

            // #513 / #493: replace the project's MIDI binding list. Lazily
            // creates `project.midi` (absent on pre-#513 projects), then
            // overwrites the bindings — caller is responsible for sending
            // the full desired list.
            Command::SaveMidiMapping { bindings } => {
                let mut project = self.project.borrow_mut();
                let midi = project.midi.get_or_insert_with(Default::default);
                midi.bindings = bindings;
                drop(project);
                Ok(vec![Event::MidiMappingSaved, Event::ProjectMutated])
            }
            other => unreachable!("handle_project received non-project command: {other:?}"),
        }
    }

    /// Side-effect for `Command::SaveProject`. Writes:
    ///
    /// 1. The canonical `.openrig` (always — `load_project_any` prefers
    ///    it on reload regardless of the user-visible path's extension).
    /// 2. The legacy `.yaml` snapshot, but only when `project_path`
    ///    points at one — keeps existing recents / shortcuts resolving.
    /// 3. The sidecar `config.yaml` with the in-project `presets_path`
    ///    pointer (currently a hardcoded `./presets`).
    /// 4. The sibling `presets/` directory so the chain-preset save
    ///    path has somewhere to write.
    fn save_project_to_disk(&self, project_path: &PathBuf) -> Result<()> {
        eprintln!("Command::SaveProject: writing to {project_path:?}");

        let parent_dir = project_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&parent_dir)
            .map_err(|e| anyhow!("failed to create project parent {parent_dir:?}: {e}"))?;

        // #555: flush any pending GUI-side rig edits back into the rig
        // before serializing. The GUI used to do this implicitly inside
        // `build_rig_for_save`; now the dispatcher owns it so every
        // transport (GUI/MCP/gRPC) gets the same up-to-date snapshot.
        // Ignore errors here — capture is best-effort and the worst
        // case is "saved without the very latest edit", same as the
        // pre-#555 GUI behaviour on dispatch failure.
        let _ = self.dispatch(Command::CaptureRigEdits);

        // Build the rig that will hit disk.
        let project_snapshot = self.project.borrow().clone();
        let rig_borrow = self.rig.borrow();
        let current_rig = rig_borrow.as_ref().map(|r| r.borrow());
        let rig_to_save = build_rig_for_save(&project_snapshot, current_rig.as_deref());
        drop(current_rig);
        drop(rig_borrow);

        let openrig_path = if project_path.extension().and_then(|e| e.to_str()) == Some("openrig") {
            project_path.clone()
        } else {
            project_path.with_extension("openrig")
        };
        infra_yaml::save_rig_project_file(&openrig_path, &rig_to_save)
            .map_err(|e| anyhow!("failed to write {openrig_path:?}: {e}"))?;

        // Legacy `.yaml` sidecar — only when the user-visible path
        // isn't already the `.openrig` itself.
        if openrig_path != *project_path {
            let legacy = infra_yaml::serialize_project(&project_snapshot)
                .map_err(|e| anyhow!("failed to serialize legacy snapshot: {e}"))?;
            std::fs::write(project_path, legacy)
                .map_err(|e| anyhow!("failed to write {project_path:?}: {e}"))?;
        }

        // Sidecar config.yaml (the in-project pointer to the preset
        // library). Uses the GUI-attached override when present;
        // otherwise derives from the project parent dir.
        let config_path = self
            .config_path
            .borrow()
            .clone()
            .unwrap_or_else(|| parent_dir.join("config.yaml"));
        let config_yaml = "presets_path: ./presets\n";
        std::fs::write(&config_path, config_yaml)
            .map_err(|e| anyhow!("failed to write {config_path:?}: {e}"))?;
        std::fs::create_dir_all(parent_dir.join("presets"))
            .map_err(|e| anyhow!("failed to create sibling presets dir: {e}"))?;
        Ok(())
    }
}
