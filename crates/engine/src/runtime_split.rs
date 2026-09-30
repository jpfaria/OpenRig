//! Responsibility: routes the modules that run a chain split.
//!
//! A Split → Mix is DSP inside ONE segment (#328, spec §4.1): path B runs in
//! the split's own buffer, never in another segment or runtime, so the
//! stream-isolation law holds by construction.

// Task 12 wires the math into the live processor and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_mix.rs"]
pub(crate) mod mix;
