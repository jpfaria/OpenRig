//! Responsibility: handles the global mixer commands.
//! #1007: validate the strip, clamp the fader, remember it, push the linear
//! gain into the engine's per-endpoint slot, persist, report.
//!
//! **Isolation (`CLAUDE.md` LAW).** A strip addresses ONE physical endpoint.
//! The engine keeps one atomic per endpoint and every stream reads only its
//! own; nothing here mixes, selects by rate or touches another endpoint. A
//! mono input with several channels runs one pipeline per channel, so its
//! strip reaches each of those pipelines' endpoint keys.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{bail, Result};

use domain::io_binding::ChannelMode;
use domain::mixer_gain::{clamp_gain_db, strip_linear_gain};
use domain::mixer_solo::solo_silenced;
use domain::mixer_strip::MixerStripId;
use domain::mixer_strips::strips_from_bindings;
use infra_filesystem::MixerStripConfig;

use crate::app_config_persist::persist_mixer_strip;
use crate::command::{Command, MixerCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::mixer_state::{MixerControlState, MixerStripSetting};
use crate::mixer_view::{mixer_view, MixerStripView};

impl LocalDispatcher {
    /// #1007: adopt the mixer state the frontend restored and apply every
    /// stored strip to the engine. Idempotent.
    pub fn attach_mixer_state(&self, state: Rc<RefCell<MixerControlState>>) {
        *self.mixer.borrow_mut() = state;
        self.apply_all_mixer_strips();
    }

    pub(crate) fn mixer_strips(&self) -> Vec<MixerStripView> {
        let state = self.mixer.borrow().clone();
        let state = state.borrow();
        mixer_view(&self.current_io_bindings(), &state)
    }

    /// Push every strip to the engine again — the stored ones and every one
    /// the bindings expose (a solo silences strips nobody ever moved). After
    /// the bindings changed, a strip's channel mode (and so its fan-out) may
    /// have too.
    pub(crate) fn apply_all_mixer_strips(&self) {
        let state = self.mixer.borrow().clone();
        let mut ids: Vec<MixerStripId> = strips_from_bindings(&self.current_io_bindings())
            .into_iter()
            .map(|strip| strip.id)
            .collect();
        for (wire, _) in state.borrow().entries() {
            if let Some(id) = MixerStripId::parse(&wire) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        for id in ids {
            let setting = state.borrow().get(&id.to_wire());
            self.apply_mixer_strip(&id, setting);
        }
    }

    pub(crate) fn handle_mixer(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Mixer(cmd) = cmd else {
            unreachable!("handle_mixer received a non-mixer command: {cmd:?}");
        };
        let (strip, change): (String, Box<dyn FnOnce(&mut MixerStripSetting)>) = match cmd {
            MixerCommand::SetMixerFader { strip, gain_db } => {
                let gain_db = clamp_gain_db(gain_db);
                (strip, Box::new(move |s| s.gain_db = gain_db))
            }
            MixerCommand::SetMixerMute { strip, muted } => {
                (strip, Box::new(move |s| s.muted = muted))
            }
            MixerCommand::ToggleMixerMute { strip } => (strip, Box::new(|s| s.muted = !s.muted)),
            MixerCommand::SetMixerSolo { strip, soloed } => {
                (strip, Box::new(move |s| s.soloed = soloed))
            }
            MixerCommand::ToggleMixerSolo { strip } => (strip, Box::new(|s| s.soloed = !s.soloed)),
        };
        let Some(id) = MixerStripId::parse(&strip) else {
            bail!("unknown mixer strip {strip:?}: expected in:<channels>@<device> or out:<channels>@<device>");
        };
        let state = self.mixer.borrow().clone();
        let before = state.borrow().get(&strip);
        let mut setting = before;
        change(&mut setting);
        state.borrow_mut().set(&strip, setting);
        if setting.soloed != before.soloed {
            // A solo changes what the whole side hears, not just this strip.
            self.apply_all_mixer_strips();
        } else {
            self.apply_mixer_strip(&id, setting);
        }
        if let Some(path) = state.borrow().config_path() {
            persist_mixer_strip(
                path,
                MixerStripConfig {
                    id: strip.clone(),
                    gain_db: setting.gain_db,
                    muted: setting.muted,
                    soloed: setting.soloed,
                },
            );
        }
        Ok(vec![Event::MixerStripChanged {
            strip,
            gain_db: setting.gain_db,
            muted: setting.muted,
            soloed: setting.soloed,
        }])
    }

    /// Write the strip's linear gain into every engine endpoint slot it
    /// feeds. A strip silenced by a solo on its side writes 0 and keeps its
    /// fader. Control thread only — the audio thread reads the atomics.
    fn apply_mixer_strip(&self, id: &MixerStripId, setting: MixerStripSetting) {
        let mode = self.mixer_strip_mode(id);
        let group_has_solo = self.mixer.borrow().borrow().group_has_solo(id.direction);
        let silent = setting.muted || solo_silenced(setting.soloed, group_has_solo);
        let linear = strip_linear_gain(setting.gain_db, silent);
        for channels in id.runtime_channel_groups(mode) {
            engine::mixer_gains::set_endpoint_gain(id.direction, &id.device_id, &channels, linear);
        }
    }

    /// The strip's channel mode from the bindings; stereo (no fan-out) when
    /// no binding declares it.
    fn mixer_strip_mode(&self, id: &MixerStripId) -> ChannelMode {
        strips_from_bindings(&self.current_io_bindings())
            .into_iter()
            .find(|strip| &strip.id == id)
            .map_or(ChannelMode::Stereo, |strip| strip.mode)
    }

    fn current_io_bindings(&self) -> Vec<domain::io_binding::IoBinding> {
        self.io_bindings
            .borrow()
            .as_ref()
            .map(|registry| registry.borrow().clone())
            .unwrap_or_default()
    }
}
