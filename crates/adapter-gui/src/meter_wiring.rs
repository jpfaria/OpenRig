//! Per-chain IN/OUT dBFS meter wiring — issue #496 / #32 / #36.
//!
//! Lifecycle:
//! 1. On chain create/upsert, subscribe to the chain's input_tap
//!    (channel 0) and stream_tap (stream 0) via the
//!    `ProjectRuntimeController`. Store the returned SPSC ring
//!    handles in `MeterState::chains` keyed by `ChainId`.
//! 2. A Slint `Timer` running at ~30 Hz calls
//!    [`compute_meter_for_chain`] for each subscribed chain, then
//!    writes the resulting dBFS pair into the matching
//!    `ProjectChainItem` row of the `project_chains` VecModel.
//! 3. Dropped chains are pruned by `prune_dead_*_taps` on the
//!    controller (existing infra).
//!
//! Only the pure compute function is exposed at the moment so it can
//! be unit-tested without spinning up a Slint runtime or an engine
//! runtime. The Slint Timer + subscribe glue will follow once the
//! pure layer is locked in.

use std::sync::Arc;

use engine::output_meter::{pop_peak_dbfs, SILENT_DBFS};
use engine::spsc::SpscRing;

/// Drain the current windows of a chain's input and output taps and
/// return `(input_peak_dbfs, output_peak_dbfs)`. Either side reports
/// [`SILENT_DBFS`] when its rings are empty.
///
/// Pure over the supplied rings — no Slint, no engine runtime,
/// directly testable.
pub fn compute_meter_for_chain(
    input_rings: &[Arc<SpscRing<f32>>],
    output_rings: &[Arc<SpscRing<f32>>],
) -> (f32, f32) {
    let i = if input_rings.is_empty() {
        SILENT_DBFS
    } else {
        pop_peak_dbfs(input_rings)
    };
    let o = if output_rings.is_empty() {
        SILENT_DBFS
    } else {
        pop_peak_dbfs(output_rings)
    };
    (i, o)
}

#[cfg(test)]
#[path = "meter_wiring_tests.rs"]
mod tests;
