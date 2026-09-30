//! Responsibility: routes the modules that run a chain split.
//!
//! A Split → Mix is DSP inside ONE segment (#328, spec §4.1): path B runs in
//! the split's own buffer, never in another segment or runtime, so the
//! stream-isolation law holds by construction.

// Task 16 wires the history adoption into the builder and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_align.rs"]
pub(crate) mod align;

// Task 14 builds split nodes from these knobs and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_knobs.rs"]
pub(crate) mod knobs;

// Task 14 aligns built split nodes from these sums and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_latency.rs"]
pub(crate) mod latency;

#[path = "runtime_split_process.rs"]
pub(crate) mod process;

// Task 16 wires the history adoption into the builder and removes this allow.
#[allow(dead_code)]
#[path = "runtime_split_state.rs"]
pub(crate) mod state;

#[cfg(test)]
#[path = "runtime_split_test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "runtime_split_dispatch_tests.rs"]
mod dispatch_tests;

#[path = "runtime_split_mix.rs"]
pub(crate) mod mix;
