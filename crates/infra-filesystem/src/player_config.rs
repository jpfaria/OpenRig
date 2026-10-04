//! Responsibility: describes the backing-track player settings a machine keeps for itself.
//!
//! ADR 0003 puts these in the SYSTEM `config.yaml`: how loud the backing track
//! plays and where it comes out belong to the person at the machine, not to
//! the rig. The track, speed, pitch and loop are practice choices of the
//! moment and are not kept.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerConfig {
    /// Linear, `0.0..=1.0`.
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// The chosen project output endpoint key, as for the metronome. `None`,
    /// or a key that no longer resolves, falls back to the first output.
    #[serde(default)]
    pub output_device: Option<String>,
}

fn default_volume() -> f32 {
    0.8
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            volume: default_volume(),
            output_device: None,
        }
    }
}

#[cfg(test)]
#[path = "player_config_tests.rs"]
mod tests;
