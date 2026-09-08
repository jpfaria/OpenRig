//! Responsibility: decides which taps a segment carries.

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

