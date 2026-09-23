//! Responsibility: tells whether a convolution block feeds an output route.
//!
//! #592: a route fed by an IR/cab convolver is born with its cushion already
//! filled (see `route_cushion`), so the convolver's first callbacks cannot
//! starve it. #965 decides it per ROUTE, not per chain: an insert splits the
//! chain, and its SEND is written by the segment BEFORE the insert — with the
//! cab behind the insert nothing convolves into the send. Likewise a mid
//! `Output` tap only hears the blocks before it.

use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;

use crate::segment_types::ChainSegment;

/// Whether `block` is an enabled convolution (IR / cab) block.
pub(crate) fn block_is_convolution(block: &AudioBlock) -> bool {
    block.enabled
        && match &block.kind {
            AudioBlockKind::Core(core) => {
                core.effect_type == block_core::EFFECT_TYPE_CAB
                    || core.effect_type == block_core::EFFECT_TYPE_IR
                    || core.model.starts_with("ir_")
            }
            AudioBlockKind::Nam(nam) => nam.model.starts_with("ir_"),
            _ => false,
        }
}

/// Whether a convolution block feeds output route `route_idx`: a segment
/// that ends in the route holds one, or a segment that taps the route
/// mid-way holds one BEFORE the tap.
pub(crate) fn route_has_convolution(
    chain: &Chain,
    segments: &[ChainSegment],
    route_idx: usize,
) -> bool {
    let convolves = |indices: &[usize]| {
        indices
            .iter()
            .filter_map(|&idx| chain.blocks.get(idx))
            .any(block_is_convolution)
    };
    segments.iter().any(|segment| {
        let at_tail =
            segment.output_route_indices.contains(&route_idx) && convolves(&segment.block_indices);
        let at_tap = segment
            .mid_output_taps
            .iter()
            .filter(|tap| tap.route_idx == route_idx)
            .any(|tap| {
                let before = tap.blocks_before.min(segment.block_indices.len());
                convolves(&segment.block_indices[..before])
            });
        at_tail || at_tap
    })
}

#[cfg(test)]
#[path = "route_convolution_tests.rs"]
mod tests;
