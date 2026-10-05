//! Responsibility: handles the drum machine commands.
//!
//! Each handler validates, records the state, persists what belongs to the
//! machine, applies the change to the frontend's drum runtime and only then
//! reports the event. The runtime is reached with no borrow of the state
//! held, because the frontend may read the dispatcher back while it works.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Result};

use infra_filesystem::DrumsConfig;

use crate::app_config_persist::persist_drums;
use crate::command::{Command, DrumsCommand};
use crate::drums_runtime::DrumsSetup;
use crate::drums_state::{DrumsControlState, DrumsSnapshot};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    pub(crate) fn drums_snapshot(&self) -> DrumsSnapshot {
        self.drums_state().borrow().snapshot()
    }

    pub(crate) fn drums_library(&self) -> crate::drums::DrumLibrary {
        self.drums_state().borrow().library().clone()
    }

    /// The drums state, cloned out of its `RefCell` so no borrow of the
    /// dispatcher outlives a call into the frontend.
    pub(crate) fn drums_state(&self) -> Rc<RefCell<DrumsControlState>> {
        self.drums.borrow().clone()
    }

    /// No attached config path means no write: a test dispatcher can never
    /// reach the user's real config.
    fn persist_drums_field(&self, mutate: impl FnOnce(&mut DrumsConfig) + Send + 'static) {
        if let Some(path) = self.drums_state().borrow().config_path() {
            persist_drums(path, mutate);
        }
    }

    /// Opens the drums' output with the current choices. A refused start
    /// returns the error and changes nothing.
    fn open_drums(&self) -> Result<()> {
        let Some(control) = self.runtime_control() else {
            return Ok(());
        };
        let Some(drums) = control.drums() else {
            return Ok(());
        };
        let (settings, output_key, kit_dir, groove) = {
            let state = self.drums_state();
            let state = state.borrow();
            let kit_dir: Option<PathBuf> = state.kit_entry().map(|k| k.dir.clone());
            (
                state.settings(),
                state.output_key().map(str::to_string),
                kit_dir,
                state.groove(),
            )
        };
        drums.start_drums(DrumsSetup {
            settings,
            output_key: output_key.as_deref(),
            kit_dir: kit_dir.as_deref(),
            groove,
        })
    }

    fn transport_event(&self, enabled: bool, playing: bool) -> Vec<Event> {
        self.drums_state()
            .borrow_mut()
            .set_transport(enabled, playing);
        vec![Event::DrumsTransportChanged { enabled, playing }]
    }

    fn content_event(&self) -> Vec<Event> {
        let snapshot = self.drums_snapshot();
        vec![Event::DrumsContentChanged {
            kit: snapshot.kit,
            groove: snapshot.groove,
        }]
    }

    pub(crate) fn settings_event(&self) -> Vec<Event> {
        let settings = self.drums_state().borrow().settings();
        if let Some(control) = self.runtime_control() {
            if let Some(drums) = control.drums() {
                drums.set_drums_settings(settings);
            }
        }
        vec![Event::DrumsSettingsChanged {
            bpm: settings.bpm,
            volume: settings.volume,
        }]
    }

    fn with_drums_runtime(&self, apply: impl FnOnce(&dyn crate::drums_runtime::DrumsRuntime)) {
        if let Some(control) = self.runtime_control() {
            if let Some(drums) = control.drums() {
                apply(drums);
            }
        }
    }

    pub(crate) fn handle_drums(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Drums(cmd) = cmd else {
            unreachable!("handle_drums received a non-drums command: {cmd:?}");
        };
        let snapshot = self.drums_snapshot();
        match cmd {
            DrumsCommand::SetDrumsEnabled { enabled: true } => {
                if !snapshot.enabled {
                    self.open_drums()?;
                }
                Ok(self.transport_event(true, snapshot.playing))
            }
            DrumsCommand::SetDrumsEnabled { enabled: false } => {
                self.with_drums_runtime(|drums| drums.stop_drums());
                Ok(self.transport_event(false, false))
            }
            DrumsCommand::PlayDrums => {
                if !snapshot.enabled {
                    self.open_drums()?;
                }
                self.with_drums_runtime(|drums| drums.set_drums_playing(true));
                Ok(self.transport_event(true, true))
            }
            DrumsCommand::StopDrums => {
                self.with_drums_runtime(|drums| drums.set_drums_playing(false));
                Ok(self.transport_event(snapshot.enabled, false))
            }
            DrumsCommand::ToggleDrums => {
                let next = if snapshot.playing {
                    DrumsCommand::StopDrums
                } else {
                    DrumsCommand::PlayDrums
                };
                self.handle_drums(Command::Drums(next))
            }
            DrumsCommand::TriggerDrumFill => {
                if !snapshot.playing {
                    bail!("a fill needs the drums playing");
                }
                self.with_drums_runtime(|drums| drums.trigger_drum_fill());
                Ok(vec![Event::DrumFillTriggered])
            }
            DrumsCommand::SetDrumsVolume { volume } => {
                let volume = self.drums_state().borrow_mut().set_volume(volume);
                self.persist_drums_field(move |config| config.volume = volume);
                Ok(self.settings_event())
            }
            DrumsCommand::SelectDrumKit { kit } => {
                let Some(dir) = self
                    .drums_state()
                    .borrow()
                    .find_kit(&kit)
                    .map(|k| k.dir.clone())
                else {
                    bail!("unknown drum kit '{kit}'");
                };
                self.drums_state().borrow_mut().set_kit(kit.clone());
                self.persist_drums_field(move |config| config.kit = Some(kit));
                self.with_drums_runtime(|drums| drums.set_drum_kit(&dir));
                Ok(self.content_event())
            }
            DrumsCommand::SelectDrumGroove { groove } => {
                let Some(found) = self.drums_state().borrow().find_groove(&groove) else {
                    bail!("unknown drum groove '{groove}'");
                };
                self.drums_state().borrow_mut().set_groove(groove.clone());
                self.persist_drums_field(move |config| config.groove = Some(groove));
                self.with_drums_runtime(|drums| drums.set_drum_groove(found));
                Ok(self.content_event())
            }
            DrumsCommand::SetDrumsOutput { output_key } => {
                self.drums_state()
                    .borrow_mut()
                    .set_output_key(output_key.clone());
                let persisted = output_key.clone();
                self.persist_drums_field(move |config| config.output_device = persisted);
                if let Some(control) = self.runtime_control() {
                    if let Some(drums) = control.drums() {
                        drums.refresh_drums_output(output_key.as_deref())?;
                    }
                }
                Ok(vec![Event::DrumsOutputChanged { output_key }])
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_drums_tests.rs"]
mod tests;
