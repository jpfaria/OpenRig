//! Responsibility: names the global mixer commands.
//! Issue #1007 — one fader and one mute per configured I/O endpoint.
//!
//! A strip is addressed by its wire id, `in:<channels>@<device>` or
//! `out:<channels>@<device>` (see `domain::mixer_strip::MixerStripId`), the
//! same id `openrig://mixer` lists. The strips are system-level: they follow
//! the machine's I/O bindings, not a chain. A chain's own faders (the
//! `Chain*` variants) sit on top of them, one chain at a time.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::ChainId;

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

    // ── A chain's own mixer (compact chain view) ─────────────────────────────
    /// Move `chain`'s own fader on one endpoint it plays through. `strip` is
    /// the global strip id of that endpoint; the chain fader multiplies with
    /// the global one and never changes it or any other chain. Clamped to
    /// `-60..=+12` dB. Project data: it travels with `project.yaml`.
    SetChainMixerFader {
        chain: ChainId,
        strip: String,
        gain_db: f32,
    },

    /// Mute or unmute `chain`'s own fader on one endpoint.
    SetChainMixerMute {
        chain: ChainId,
        strip: String,
        muted: bool,
    },

    /// Flip `chain`'s own mute on one endpoint.
    ToggleChainMixerMute { chain: ChainId, strip: String },

    /// Move `chain`'s DI-loop fader. Clamped to `-60..=+12` dB.
    SetChainDiFader { chain: ChainId, gain_db: f32 },
}
