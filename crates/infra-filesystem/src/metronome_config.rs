//! Per-machine metronome settings (#14).
//!
//! ADR 0003 puts these in the SYSTEM `config.yaml`: a practice tempo belongs
//! to the person at the machine, not to the rig, so it must not travel inside
//! a `.openrig`.
//!
//! There is deliberately no `enabled` field. The metronome always boots off,
//! and leaving the flag out of the persisted shape means no code path can
//! ever write it — a session can never start clicking on its own.
//!
//! `subdivision` and `timbre` are stored as strings so the config format does
//! not bind to a DSP enum's discriminants; the dispatcher validates them
//! before they get here.

use serde::{Deserialize, Serialize};

/// Defaults mirror `feature_dsp::metronome::MetronomeSettings::default()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetronomeConfig {
    #[serde(default = "default_bpm")]
    pub bpm: f32,
    #[serde(default = "default_beats_per_bar")]
    pub beats_per_bar: u32,
    /// `off` | `eighths` | `triplets` | `sixteenths`.
    #[serde(default = "default_subdivision")]
    pub subdivision: String,
    /// `click` | `wood` | `beep`.
    #[serde(default = "default_timbre")]
    pub timbre: String,
    /// Linear, `0.0..=1.0`.
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub count_in: bool,
    /// Device the metronome's own output stream opens. `None` follows the
    /// system default output.
    #[serde(default)]
    pub output_device: Option<String>,
}

fn default_bpm() -> f32 {
    120.0
}

fn default_beats_per_bar() -> u32 {
    4
}

fn default_subdivision() -> String {
    "off".to_string()
}

fn default_timbre() -> String {
    "click".to_string()
}

fn default_volume() -> f32 {
    0.7
}

impl Default for MetronomeConfig {
    fn default() -> Self {
        Self {
            bpm: default_bpm(),
            beats_per_bar: default_beats_per_bar(),
            subdivision: default_subdivision(),
            timbre: default_timbre(),
            volume: default_volume(),
            count_in: false,
            output_device: None,
        }
    }
}
