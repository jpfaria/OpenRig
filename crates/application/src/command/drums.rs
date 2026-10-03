//! Responsibility: names the drum machine commands.
//!
//! The drum machine is a global practice tool, not part of any chain, and it
//! plays through its own output stream. Kits and grooves travel by id; the
//! dispatcher checks them against the installed library.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Every drum machine state change any controller can request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum DrumsCommand {
    /// Open (`true`) or close (`false`) the drums' output. Closing also stops
    /// the groove. Not persisted: the app always opens with the drums off.
    SetDrumsEnabled { enabled: bool },

    /// Start the groove from its first beat, opening the output if needed.
    PlayDrums,

    /// Stop the groove; the output stays open.
    StopDrums,

    /// Play when stopped, stop when playing — one footswitch for both.
    ToggleDrums,

    /// Play the groove's next fill; it lands on the next bar.
    TriggerDrumFill,

    /// Set the tempo. Clamped by the dispatcher to the supported range.
    SetDrumsBpm { bpm: f32 },

    /// Drums level, `0.0..=1.0`, independent of any chain volume.
    SetDrumsVolume { volume: f32 },

    /// Pick a kit by id from the installed library.
    SelectDrumKit { kit: String },

    /// Pick a groove by id from the installed library.
    SelectDrumGroove { groove: String },

    /// Which output endpoint the drums play through. `None` falls back to the
    /// project's first output.
    SetDrumsOutput { output_key: Option<String> },
}
