//! Issue #14 — the built-in metronome.
//!
//! The metronome is a global utility, not part of any chain, and it plays
//! through its own output stream. These commands only carry intent: the adapter
//! owns the stream's lifecycle, exactly as it does for the Tuner.
//!
//! `subdivision` and `timbre` travel as strings rather than as the `feature-dsp`
//! enums so this crate stays free of a DSP dependency; the dispatcher parses
//! them and rejects anything it does not recognize.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Every metronome state change any controller can request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum MetronomeCommand {
    /// Start or stop the click. Not persisted — the app always opens with the
    /// metronome off.
    SetMetronomeEnabled { enabled: bool },

    /// Set the tempo. Clamped by the dispatcher to the supported range.
    SetMetronomeBpm { bpm: f32 },

    /// Beats per bar; beat 1 takes the accent.
    SetMetronomeTimeSignature { beats_per_bar: u32 },

    /// `"off"`, `"eighths"`, `"triplets"` or `"sixteenths"`.
    SetMetronomeSubdivision { subdivision: String },

    /// Click level, `0.0..=1.0`, independent of any chain volume.
    SetMetronomeVolume { volume: f32 },

    /// `"click"`, `"wood"` or `"beep"`.
    SetMetronomeTimbre { timbre: String },

    /// Whether starting the metronome plays one count-in bar first.
    SetMetronomeCountIn { enabled: bool },

    /// Which output device the click plays through. `None` clears the choice
    /// and falls back to the system default.
    SetMetronomeOutput { device_id: Option<String> },

    /// One tap of the tap-tempo button. The adapter keeps the tap history and
    /// dispatches the resulting `SetMetronomeBpm`; this records the tap so MIDI
    /// and MCP reach tap tempo through the same door as the GUI.
    MetronomeTap,
}
