//! #14 — the metronome commands, and (#127) the state they act on.
//!
//! The dispatcher owns the metronome: it validates the payload, remembers the
//! setting, persists it to the per-machine `config.yaml`, applies it to the
//! frontend's audio runtime through
//! [`crate::runtime_control::RuntimeControl`], and only then reports the event.
//!
//! Before #127 only the first of those happened here. The runtime effect lived
//! in the GUI's `metronome_events::apply_events`, reachable from a knob
//! callback and nowhere else — so a MIDI footswitch bound to
//! `toggle_metronome`, or an MCP client calling `set_metronome_enabled`,
//! flipped the mirror in `SelectionState`, received
//! `Event::MetronomeEnabledChanged`, and the click never played. Same for the
//! settings: they reached the generator only because the GUI's own handler
//! pushed them.
//!
//! **Isolation (`CLAUDE.md` LAW).** The click is an independent pipeline
//! (invariant #4): its own generator, its own output stream, summed by the
//! backend. Nothing here addresses a chain, and the endpoint it plays through
//! travels as an opaque KEY — this crate never learns about devices.

use anyhow::{bail, Result};

use feature_dsp::metronome::{Subdivision, Timbre, BPM_MAX, BPM_MIN};

use infra_filesystem::MetronomeConfig;

use crate::app_config_persist::persist_metronome;
use crate::command::{Command, MetronomeCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::metronome_state::MetronomeSnapshot;

/// The beat lamps stop at sixteen, and a bar of zero beats has no meaning.
const BEATS_PER_BAR_MIN: u32 = 1;
const BEATS_PER_BAR_MAX: u32 = 16;

impl LocalDispatcher {
    pub(crate) fn metronome_snapshot(&self) -> MetronomeSnapshot {
        self.metronome_state().borrow().snapshot()
    }

    /// Read-modify-write one field of the metronome section of the
    /// per-machine `config.yaml`.
    ///
    /// No attached config path ⇒ no write. That is the whole guard against
    /// #701: a dispatcher a test built has nowhere to persist to, so it can
    /// never reach the user's real config.
    fn persist_metronome_field(&self, mutate: impl FnOnce(&mut MetronomeConfig) + Send + 'static) {
        let path = self.metronome_state().borrow().config_path();
        if path.is_some() {
            persist_metronome(path, mutate);
        }
    }

    /// Hand the current settings to a click that may be running.
    ///
    /// Cheap and idempotent: the shared cell bumps a generation counter and
    /// the audio callback only re-reads when it changed — a tempo edit never
    /// restarts the stream, so a live edit cannot drop audio.
    fn push_metronome_settings(&self) {
        let settings = self.metronome_state().borrow().settings();
        if let Some(control) = self.runtime_control() {
            control.set_metronome_settings(settings);
        }
    }

    pub(crate) fn handle_metronome(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Metronome(cmd) = cmd else {
            unreachable!("handle_metronome received a non-metronome command: {cmd:?}");
        };
        match cmd {
            MetronomeCommand::SetMetronomeEnabled { enabled } => {
                // Take the settings and the endpoint in one borrow that ends
                // BEFORE the runtime is touched: the frontend's control reads
                // the dispatcher back while it resolves the endpoint.
                let state = self.metronome_state();
                let (settings, output_key) = {
                    let metronome = state.borrow();
                    (
                        metronome.settings(),
                        metronome.output_key().map(str::to_string),
                    )
                };
                if let Some(control) = self.runtime_control() {
                    if enabled {
                        // `?`: a start the runtime refused made no sound, so
                        // nothing below runs and the state keeps saying the
                        // click is stopped — which it is.
                        control.start_metronome(settings, output_key.as_deref())?;
                    } else {
                        control.stop_metronome();
                    }
                }
                state.borrow_mut().set_running(enabled);
                // Mirror into the snapshot so the MIDI slot `toggle_metronome`
                // knows what to flip on the next press (same as the tuner).
                if let Ok(mut selection) = self.selection_state.write() {
                    selection.metronome_enabled = enabled;
                }
                Ok(vec![Event::MetronomeEnabledChanged { enabled }])
            }

            MetronomeCommand::SetMetronomeBpm { bpm } => {
                let bpm = bpm.clamp(BPM_MIN, BPM_MAX);
                self.metronome_state()
                    .borrow_mut()
                    .update_settings(|settings| settings.bpm = bpm);
                self.persist_metronome_field(move |config| config.bpm = bpm);
                self.push_metronome_settings();
                Ok(vec![Event::MetronomeBpmChanged { bpm }])
            }

            MetronomeCommand::SetMetronomeTimeSignature { beats_per_bar } => {
                let beats_per_bar = beats_per_bar.clamp(BEATS_PER_BAR_MIN, BEATS_PER_BAR_MAX);
                self.metronome_state()
                    .borrow_mut()
                    .update_settings(|settings| settings.beats_per_bar = beats_per_bar);
                self.persist_metronome_field(move |config| config.beats_per_bar = beats_per_bar);
                self.push_metronome_settings();
                Ok(vec![Event::MetronomeTimeSignatureChanged { beats_per_bar }])
            }

            MetronomeCommand::SetMetronomeVolume { volume } => {
                let volume = volume.clamp(0.0, 1.0);
                self.metronome_state()
                    .borrow_mut()
                    .update_settings(|settings| settings.volume = volume);
                self.persist_metronome_field(move |config| config.volume = volume);
                self.push_metronome_settings();
                Ok(vec![Event::MetronomeVolumeChanged { volume }])
            }

            MetronomeCommand::SetMetronomeSubdivision { subdivision } => {
                // An unknown enum string must never silently become a
                // different sound — it is an error, not a fallback.
                let Some(parsed) = Subdivision::from_key(&subdivision) else {
                    bail!(
                        "unknown metronome subdivision '{subdivision}' (expected one of {})",
                        Subdivision::KEYS.join(", ")
                    );
                };
                self.metronome_state()
                    .borrow_mut()
                    .update_settings(|settings| settings.subdivision = parsed);
                let persisted = subdivision.clone();
                self.persist_metronome_field(move |config| config.subdivision = persisted);
                self.push_metronome_settings();
                Ok(vec![Event::MetronomeSubdivisionChanged { subdivision }])
            }

            MetronomeCommand::SetMetronomeTimbre { timbre } => {
                let Some(parsed) = Timbre::from_key(&timbre) else {
                    bail!(
                        "unknown metronome timbre '{timbre}' (expected one of {})",
                        Timbre::KEYS.join(", ")
                    );
                };
                self.metronome_state()
                    .borrow_mut()
                    .update_settings(|settings| settings.timbre = parsed);
                let persisted = timbre.clone();
                self.persist_metronome_field(move |config| config.timbre = persisted);
                self.push_metronome_settings();
                Ok(vec![Event::MetronomeTimbreChanged { timbre }])
            }

            MetronomeCommand::SetMetronomeCountIn { enabled } => {
                self.metronome_state()
                    .borrow_mut()
                    .update_settings(|settings| settings.count_in = enabled);
                self.persist_metronome_field(move |config| config.count_in = enabled);
                self.push_metronome_settings();
                Ok(vec![Event::MetronomeCountInChanged { enabled }])
            }

            MetronomeCommand::SetMetronomeOutput { device_id } => {
                // `device_id` carries the output-ENDPOINT key (#14). It stays
                // opaque here: only the frontend that owns the audio host knows
                // which device and channels it names.
                self.metronome_state()
                    .borrow_mut()
                    .set_output_key(device_id.clone());
                let persisted = device_id.clone();
                self.persist_metronome_field(move |config| config.output_device = persisted);
                // A running click follows the new endpoint immediately; a
                // stopped one simply opens there next time. Picking is not
                // playing, so this must never start a runtime.
                if let Some(control) = self.runtime_control() {
                    control.refresh_metronome_output(device_id.as_deref())?;
                }
                Ok(vec![Event::MetronomeOutputChanged { device_id }])
            }

            MetronomeCommand::MetronomeTap => {
                let bpm = self
                    .metronome_state()
                    .borrow_mut()
                    .tap_at(std::time::Instant::now());
                let mut events = vec![Event::MetronomeTapped];
                // The tempo the taps imply travels as a Command of its own, so
                // every observer sees it exactly as if it had been set by hand
                // — including the persist and the push to the generator.
                if let Some(bpm) = bpm {
                    events.extend(self.handle_metronome(Command::Metronome(
                        MetronomeCommand::SetMetronomeBpm { bpm },
                    ))?);
                }
                Ok(events)
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_metronome_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "local_dispatcher_metronome_runtime_tests.rs"]
mod runtime_tests;
