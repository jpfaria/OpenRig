//! DSP utilities for OpenRig top-level features (Tuner, Spectrum Analyzer, ...).
//!
//! These are not audio blocks — they run on UI/worker threads and read sample
//! taps from the engine. Block-level DSP lives in the `block-*` crates.

pub mod pitch_yin;
