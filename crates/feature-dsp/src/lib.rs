//! DSP utilities for OpenRig top-level features (Tuner, Spectrum Analyzer, ...).
//!
//! These are not audio blocks — they run on UI/worker threads and read sample
//! taps from the engine. Block-level DSP lives in the `block-*` crates.

pub mod metronome;
pub mod pitch_yin;
pub mod quality_metrics;
pub mod spectrum_fft;
pub mod tone_descriptors;
pub mod tone_profiles;
