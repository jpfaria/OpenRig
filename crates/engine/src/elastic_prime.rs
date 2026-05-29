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

use ir::PARTITION_SIZE;
use project::block::AudioBlockKind;
use project::chain::Chain;

/// Whether `chain` has an enabled convolution (IR / cab) block — the only
/// block kind whose per-partition FFT spike warrants the cushion.
pub(crate) fn chain_has_convolution(chain: &Chain) -> bool {
    chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .any(|b| match &b.kind {
            AudioBlockKind::Core(core) => {
                core.effect_type == block_core::EFFECT_TYPE_CAB
                    || core.effect_type == block_core::EFFECT_TYPE_IR
                    || core.model.starts_with("ir_")
            }
            AudioBlockKind::Nam(nam) => nam.model.starts_with("ir_"),
            _ => false,
        })
}

/// Output elastic-buffer capacity target. IR chains floor at one convolver
/// partition of headroom; everyone else keeps the device-derived `base`.
pub(crate) fn elastic_capacity_target(base: usize, has_convolution: bool) -> usize {
    if has_convolution {
        base.max(PARTITION_SIZE)
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
