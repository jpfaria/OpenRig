//! Core building blocks shared by OpenRig block families.
//!
//! Phase 6 of issue #194: this crate's surface is split across topical
//! sub-modules so adding a new effect-type constant or DSP helper goes to
//! the right file without growing a god `lib.rs`. This file is now
//! re-exports + module declarations only — no logic.

pub mod audio_types;
pub mod brand_visual;
pub mod constants;
pub mod dsp;
pub mod param;
pub mod traits;
pub mod visual;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

// Re-exports so existing `block_core::*` callers (every block-* crate,
// engine, infra-cpal, adapter-gui) keep working unchanged.
pub use audio_types::{
    new_stream_handle, AudioChannelLayout, ModelAudioMode, StreamEntry, StreamHandle,
};
pub use constants::{
    ALL_INSTRUMENTS, BRAND_NATIVE, DEFAULT_INSTRUMENT, EFFECT_TYPE_AMP, EFFECT_TYPE_BODY,
    EFFECT_TYPE_CAB, EFFECT_TYPE_DELAY, EFFECT_TYPE_DYNAMICS, EFFECT_TYPE_FILTER,
    EFFECT_TYPE_FULL_RIG, EFFECT_TYPE_GAIN, EFFECT_TYPE_IR, EFFECT_TYPE_MODULATION,
    EFFECT_TYPE_NAM, EFFECT_TYPE_PITCH, EFFECT_TYPE_PREAMP, EFFECT_TYPE_REVERB,
    EFFECT_TYPE_UTILITY, EFFECT_TYPE_VST3, EFFECT_TYPE_WAH, GUITAR_ACOUSTIC_BASS, GUITAR_BASS,
    INST_ACOUSTIC_GUITAR, INST_BASS, INST_DRUMS, INST_ELECTRIC_GUITAR, INST_GENERIC, INST_KEYS,
    INST_VOICE,
};
pub use dsp::{
    calculate_coefficient, capitalize_first, db_to_lin, lin_to_db, BiquadFilter, BiquadKind,
    EnvelopeFollower, OnePoleHighPass, OnePoleLowPass,
};
pub use traits::{BlockProcessor, MonoProcessor, NamedModel, PluginEditorHandle, StereoProcessor};
pub use brand_visual::{brand_colors, compose, ModelColorOverride, ModelColorScheme};
pub use visual::{KnobLayoutEntry, ModelVisualData};
