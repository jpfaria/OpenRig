//! Responsibility: primes the output cushion before audio starts flowing.
//! Issue #592 — output elastic-buffer cushion for convolution (IR) chains.
//!
//! An IR cab runs a full FFT inline once per `ir::PARTITION_SIZE` samples,
//! so at small device buffers that periodic spike can momentarily starve
//! the output before the DSP producer warms up — the freshly-loaded preset
//! crackles/distorts until a warm rebuild. The fix gives such chains a real
//! jitter cushion: the output elastic buffer is sized to hold at least one
//! convolver partition AND primed with that much silence on the INITIAL
//! build (a rebuild runs warm and refills naturally, so it is not primed).
//!
//! These are pure helpers so the policy is testable in isolation from the
//! runtime assembly.

use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;

use crate::segment_types::ChainSegment;

/// Output elastic-buffer cushion (frames) primed for IR/convolution chains
/// on a cold start.
///
/// Historically this equalled the convolver's partition size: the old
/// convolver ran its whole per-partition FFT as one burst every
/// `ir::PARTITION_SIZE` samples, and the cushion had to cover that spike.
/// Issue #617 made the convolver spread that work evenly across callbacks
/// (`ir::PARTITION_SIZE` dropped to 64), removing the spike at the source.
/// The cushion is now decoupled from the partition size and kept at the
/// proven #592 magnitude purely to absorb generic first-callback producer
/// warmup jitter at small device buffers — not the (now eliminated) spike.
pub(crate) const IR_COLD_START_CUSHION_FRAMES: usize = 512;

/// Whether `block` is an enabled convolution (IR / cab) block — the only
/// block kind whose per-partition FFT spike warrants the cushion.
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

/// Whether a convolution block feeds output route `route_idx`: one of the
/// segments writing that route (at its tail or through a mid tap) holds one.
///
/// #965: decided per ROUTE, not per chain. An insert splits the chain, and
/// its SEND is written by the segment BEFORE the insert; with the cab behind
/// the insert nothing convolves into the send, and the cushion there was pure
/// loop latency — it undid the lean send target the cpal layer computes.
pub(crate) fn route_has_convolution(
    chain: &Chain,
    segments: &[ChainSegment],
    route_idx: usize,
) -> bool {
    segments
        .iter()
        .filter(|segment| {
            segment.output_route_indices.contains(&route_idx)
                || segment
                    .mid_output_taps
                    .iter()
                    .any(|tap| tap.route_idx == route_idx)
        })
        .any(|segment| {
            segment
                .block_indices
                .iter()
                .filter_map(|&idx| chain.blocks.get(idx))
                .any(block_is_convolution)
        })
}

/// Cold-start cushion for a route: how much silence an IR route is primed
/// with (and how much its ring must hold on top of the resting level).
/// #965: this is NOT the route's resting target any more — the route rests at
/// `base` and the drift guard sheds the prime once the route runs clean.
/// IR routes floor at the #592 cushion; everyone else keeps `base`.
pub(crate) fn elastic_capacity_target(base: usize, has_convolution: bool) -> usize {
    if has_convolution {
        base.max(IR_COLD_START_CUSHION_FRAMES)
    } else {
        base
    }
}

/// Silence cushion to prime the output buffer with. Only the INITIAL build
/// of an IR chain is primed (rebuilds run warm); everything else is 0.
pub(crate) fn elastic_prime_frames(
    target: usize,
    is_initial_build: bool,
    has_convolution: bool,
) -> usize {
    if is_initial_build && has_convolution {
        target
    } else {
        0
    }
}

#[cfg(test)]
#[path = "elastic_prime_tests.rs"]
mod tests;
