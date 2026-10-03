//! Responsibility: describes the drum machine settings a machine keeps for itself.
//!
//! These live in the system `config.yaml` (ADR 0003): a practice groove
//! belongs to the person at the machine, not to the rig. There is no
//! `playing` field, so no code path can make a session boot with drums
//! running.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrumsConfig {
    #[serde(default = "default_bpm")]
    pub bpm: f32,
    /// Linear, `0.0..=1.0`.
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// The chosen kit id; `None`, or an id no longer installed, falls back to
    /// the first kit.
    #[serde(default)]
    pub kit: Option<String>,
    /// The chosen groove id, with the same fallback as `kit`.
    #[serde(default)]
    pub groove: Option<String>,
    /// The chosen project output endpoint key, as the metronome stores it.
    #[serde(default)]
    pub output_device: Option<String>,
}

fn default_bpm() -> f32 {
    120.0
}

fn default_volume() -> f32 {
    0.8
}

impl Default for DrumsConfig {
    fn default() -> Self {
        Self {
            bpm: default_bpm(),
            volume: default_volume(),
            kit: None,
            groove: None,
            output_device: None,
        }
    }
}

#[cfg(test)]
#[path = "drums_config_tests.rs"]
mod tests;
