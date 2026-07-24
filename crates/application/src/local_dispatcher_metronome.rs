//! #14 — the metronome commands. Same shape as the Tuner (`SetTunerEnabled`):
//! the adapter owns the stream's lifecycle, the dispatcher records the intent
//! and signals an event, so GUI, MIDI, MCP and gRPC all come through one door.
//!
//! What the dispatcher does own is validation. Every one of these can arrive
//! from an MCP client or a MIDI CC, so the range clamps and the enum parsing
//! live here — an out-of-range tempo must never reach the audio thread.

use anyhow::{bail, Result};

use crate::command::{Command, MetronomeCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

/// Slowest and fastest tempo the generator supports. Mirrors
/// `feature_dsp::metronome::BPM_MIN`/`BPM_MAX`; kept as literals so this crate
/// does not take a DSP dependency for two numbers.
const BPM_MIN: f32 = 30.0;
const BPM_MAX: f32 = 300.0;

/// The beat lamps stop at sixteen, and a bar of zero beats has no meaning.
const BEATS_PER_BAR_MIN: u32 = 1;
const BEATS_PER_BAR_MAX: u32 = 16;

const SUBDIVISIONS: [&str; 4] = ["off", "eighths", "triplets", "sixteenths"];
const TIMBRES: [&str; 3] = ["click", "wood", "beep"];

impl LocalDispatcher {
    pub(crate) fn handle_metronome(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Metronome(cmd) = cmd else {
            unreachable!("handle_metronome received a non-metronome command: {cmd:?}");
        };
        match cmd {
            MetronomeCommand::SetMetronomeEnabled { enabled } => {
                // Mirror into the snapshot so the MIDI slot `toggle_metronome`
                // knows what to flip on the next press (same as the tuner).
                if let Ok(mut state) = self.selection_state.write() {
                    state.metronome_enabled = enabled;
                }
                Ok(vec![Event::MetronomeEnabledChanged { enabled }])
            }

            MetronomeCommand::SetMetronomeBpm { bpm } => Ok(vec![Event::MetronomeBpmChanged {
                bpm: bpm.clamp(BPM_MIN, BPM_MAX),
            }]),

            MetronomeCommand::SetMetronomeTimeSignature { beats_per_bar } => {
                Ok(vec![Event::MetronomeTimeSignatureChanged {
                    beats_per_bar: beats_per_bar.clamp(BEATS_PER_BAR_MIN, BEATS_PER_BAR_MAX),
                }])
            }

            MetronomeCommand::SetMetronomeVolume { volume } => {
                Ok(vec![Event::MetronomeVolumeChanged {
                    volume: volume.clamp(0.0, 1.0),
                }])
            }

            MetronomeCommand::SetMetronomeSubdivision { subdivision } => {
                if !SUBDIVISIONS.contains(&subdivision.as_str()) {
                    bail!(
                        "unknown metronome subdivision '{subdivision}' (expected one of {})",
                        SUBDIVISIONS.join(", ")
                    );
                }
                Ok(vec![Event::MetronomeSubdivisionChanged { subdivision }])
            }

            MetronomeCommand::SetMetronomeTimbre { timbre } => {
                if !TIMBRES.contains(&timbre.as_str()) {
                    bail!(
                        "unknown metronome timbre '{timbre}' (expected one of {})",
                        TIMBRES.join(", ")
                    );
                }
                Ok(vec![Event::MetronomeTimbreChanged { timbre }])
            }

            MetronomeCommand::SetMetronomeCountIn { enabled } => {
                Ok(vec![Event::MetronomeCountInChanged { enabled }])
            }

            MetronomeCommand::SetMetronomeOutput { device_id } => {
                Ok(vec![Event::MetronomeOutputChanged { device_id }])
            }

            MetronomeCommand::MetronomeTap => Ok(vec![Event::MetronomeTapped]),
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_metronome_tests.rs"]
mod tests;
