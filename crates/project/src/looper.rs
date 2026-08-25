//! Responsibility: describes one looper attached to a chain.

use serde::{Deserialize, Serialize};

use crate::endpoint_ref::EndpointRef;

/// #323: how many loopers one chain can hold. A domain rule (it caps what a
/// project may contain), read by the engine to size its slots.
pub const LOOPER_MAX_PER_CHAIN: usize = 8;

/// #323: playback rate of a looper. Not a resample — the read cursor steps by
/// this factor, so the pitch follows the speed (classic looper behaviour).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LooperSpeed {
    Half,
    #[default]
    Normal,
    Double,
}

/// #323: one per-chain looper as it travels in `project.openrig`. The recorded
/// audio itself lives beside the project (`audio_file`), not in the YAML.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LooperConfig {
    /// Stable identity of this looper, also used by the audio thread to
    /// address its slot. Unique within the chain.
    pub uid: u64,
    /// Loop level, 0..=1.
    #[serde(default = "default_looper_mix")]
    pub mix: f32,
    /// Per-layer-of-age gain applied to older layers, 0..=1 (1 = no decay).
    #[serde(default = "default_looper_decay")]
    pub decay: f32,
    #[serde(default)]
    pub speed: LooperSpeed,
    #[serde(default)]
    pub reverse: bool,
    /// File name (relative to the project's loop folder) of the recorded
    /// mixdown, when the looper has audio saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_file: Option<String>,
    /// #323: which of the chain's bound input endpoints this looper records
    /// its dry signal from. `None` ⇒ the chain's first input (the default and
    /// the legacy behaviour — a project written before the selector existed
    /// deserializes to `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<EndpointRef>,
    /// #323: which of the chain's bound output endpoints the loop plays back
    /// to. `None` ⇒ the chain's main output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<EndpointRef>,
    /// #323 phase 2: id of the preset whose effects this loop plays through.
    /// The loop records DRY (clean) and carries a reference to WHICH preset
    /// renders it, so switching the chain's live preset to solo does not
    /// change the loop's tone. Linked to the chain's active preset on RECORD
    /// and reassignable via the drawer's picker. `None` ⇒ the chain's current
    /// preset (the pre-phase-2 behaviour and legacy projects).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
}

fn default_looper_mix() -> f32 {
    1.0
}

fn default_looper_decay() -> f32 {
    1.0
}

impl LooperConfig {
    /// A fresh, empty looper at unity level and normal speed.
    pub fn new(uid: u64) -> Self {
        Self {
            uid,
            mix: default_looper_mix(),
            decay: default_looper_decay(),
            speed: LooperSpeed::default(),
            reverse: false,
            audio_file: None,
            input: None,
            output: None,
            preset: None,
        }
    }
}
