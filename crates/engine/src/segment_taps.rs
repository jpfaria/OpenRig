//! Responsibility: decides which taps a segment carries.

use crate::runtime_endpoints::{BindingIo, InputEntry, OutputEntry};

use crate::segment_types::{MidOutputTap, SegmentTap};

/// Resolve the taps whose position falls inside `block_range` against one
/// segment's `block_indices`, converting each chain offset into "after how many
/// of THIS segment's blocks".
pub(crate) fn taps_for_segment(
    mid_taps: &[MidOutputTap],
    block_indices: &[usize],
    block_range: std::ops::Range<usize>,
) -> Vec<SegmentTap> {
    mid_taps
        .iter()
        .filter(|t| block_range.contains(&t.offset))
        .map(|t| SegmentTap {
            blocks_before: block_indices.iter().filter(|&&i| i < t.offset).count(),
            route_idx: t.route_idx,
        })
        .collect()
}

/// One segment per `(input × tail output)` pair when no enabled Insert blocks
/// exist. Model A: the chain's tail outputs come from the bindings and sit at
/// the chain END, so every effect block feeds every tail output — one segment
/// per (input, tail output) covering all enabled effect blocks. Mid `Output`
/// blocks are carried as taps on those segments (#85), never as pairs of their
/// own; for the head/tail case this stays bit-exact to the legacy
/// single-tail-output path.
pub(crate) fn binding_of_input<'a>(by: &'a [BindingIo], e: &InputEntry) -> Option<&'a str> {
    by.iter()
        .find(|b| {
            b.inputs
                .iter()
                .any(|i| i.device_id == e.device_id && i.channels == e.channels)
        })
        .map(|b| b.binding_id.as_str())
}

pub(crate) fn binding_of_output<'a>(by: &'a [BindingIo], e: &OutputEntry) -> Option<&'a str> {
    by.iter()
        .find(|b| {
            b.outputs
                .iter()
                .any(|o| o.device_id == e.device_id && o.channels == e.channels)
        })
        .map(|b| b.binding_id.as_str())
}
