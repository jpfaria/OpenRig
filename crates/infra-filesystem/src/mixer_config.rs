//! Responsibility: describes one persisted global-mixer strip.
//! Per-machine mixer settings (#1007).
//!
//! ADR 0003 puts these in the SYSTEM `config.yaml`: a monitor level belongs to
//! the desk the interface sits on, not to a `.openrig` that travels. A strip at
//! unity and unmuted is simply absent — the default needs no entry.

use serde::{Deserialize, Serialize};

/// One strip's fader and mute, keyed by its wire id (`out:0,1@<device>`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixerStripConfig {
    pub id: String,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub muted: bool,
}
