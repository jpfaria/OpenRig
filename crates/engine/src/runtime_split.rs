//! Responsibility: routes the modules that run a chain split.
//!
//! A Split → Mix is DSP inside ONE segment (#328, spec §4.1): path B runs in
//! the split's own buffer, never in another segment or runtime, so the
//! stream-isolation law holds by construction.

#[path = "runtime_split_align.rs"]
pub(crate) mod align;

#[path = "runtime_split_builder.rs"]
pub(crate) mod builder;

#[path = "runtime_split_knobs.rs"]
pub(crate) mod knobs;

#[path = "runtime_split_lanes.rs"]
pub(crate) mod lanes;

#[path = "runtime_split_latency.rs"]
pub(crate) mod latency;

#[path = "runtime_split_process.rs"]
pub(crate) mod process;

#[path = "runtime_split_state.rs"]
pub(crate) mod state;

#[path = "runtime_split_walk.rs"]
pub(crate) mod walk;

#[cfg(test)]
#[path = "runtime_split_test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "runtime_split_dispatch_tests.rs"]
mod dispatch_tests;

#[path = "runtime_split_mix.rs"]
pub(crate) mod mix;
