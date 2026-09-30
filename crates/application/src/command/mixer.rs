//! Responsibility: names the global mixer commands.
//! Issue #1007 — one fader and one mute per configured I/O endpoint.
//!
//! A strip is addressed by its wire id, `in:<channels>@<device>` or
//! `out:<channels>@<device>` (see `domain::mixer_strip::MixerStripId`), the
//! same id `openrig://mixer` lists. The strips are system-level: they follow
//! the machine's I/O bindings, not a chain.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Every mixer change any controller can request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum MixerCommand {
    /// Move a strip's fader. Clamped by the dispatcher to `-60..=+12` dB;
    /// the bottom of the travel is silence.
    SetMixerFader { strip: String, gain_db: f32 },

    /// Mute or unmute a strip. The fader position is kept.
    SetMixerMute { strip: String, muted: bool },

    /// Flip a strip's mute — the shape a surface's MUTE button sends.
    ToggleMixerMute { strip: String },

    /// Solo or unsolo a strip. While any strip of a side (inputs, or
    /// outputs) is soloed, the strips of that side NOT soloed are silent.
    /// Solos add up; the faders are never touched.
    SetMixerSolo { strip: String, soloed: bool },

    /// Flip a strip's solo — the shape a surface's SOLO button sends.
    ToggleMixerSolo { strip: String },
}
