//! Per-output elastic-buffer target sizing for a chain.
//!
//! The engine's elastic buffer absorbs jitter between the input callback
//! and the output callback. The target depth depends on:
//!
//! 1. Backend — Linux+JACK uses a worker-thread DSP path with non-RT
//!    scheduling jitter, so it needs more headroom (8x buffer) than the
//!    direct CPAL callbacks on macOS/Windows (2x buffer).
//! 2. Endpoint kind — a regular output needs the default multiplier, but
//!    an Insert send already sees post-elastic samples and is followed by
//!    external hardware that has its own driver buffering, so doubling
//!    the headroom there is pure latency overhead.
//!
//! `compute_elastic_targets_for_chain` pairs each output in
//! `ResolvedChainAudioConfig::outputs` with its right multiplier and
//! returns the per-output target depths in the same order. The caller
//! (`upsert_chain_with_resolved`) feeds them straight into
//! `RuntimeGraph::upsert_chain`.

use engine::runtime::elastic_target_for_buffer;
use project::block::AudioBlockKind;
use project::chain::Chain;

use crate::resolved::ResolvedChainAudioConfig;

/// Backend-specific multiplier for the elastic buffer target.
/// JACK uses a worker-thread DSP path on Linux; non-RT scheduling jitter
/// needs more headroom than direct CPAL callbacks.
#[cfg(all(target_os = "linux", feature = "jack"))]
const ELASTIC_MULTIPLIER: u8 = 8;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
const ELASTIC_MULTIPLIER: u8 = 2;

/// Multiplier used for the elastic target of a regular output route.
/// See `ELASTIC_MULTIPLIER` for the per-backend rationale.
const ELASTIC_MULTIPLIER_REGULAR: u8 = ELASTIC_MULTIPLIER;
/// Multiplier used for the elastic target of an Insert block's *send*
/// endpoint. The main chain's elastic buffer already absorbs upstream
/// jitter before the signal reaches the insert send, and the external
/// hardware on the other side has its own driver buffering. Keeping the
/// send's elastic at the default multiplier would be pure redundancy
/// and roughly doubles the insert's round-trip latency; `1` trims that
/// overhead while the shared `ELASTIC_TARGET_FLOOR` prevents pathologic
/// sizing for tiny device buffers.
const ELASTIC_MULTIPLIER_INSERT_SEND: u8 = 1;

/// Compute per-output elastic targets for a chain. Regular outputs use
/// the backend's default multiplier; Insert send endpoints use a leaner
/// multiplier to avoid doubling the round-trip latency of the external
/// effect loop. The order of the returned Vec matches
/// `ResolvedChainAudioConfig::outputs`, which places regular outputs
/// first and Insert sends last (mirroring `effective_outputs`).
pub(crate) fn compute_elastic_targets_for_chain(
    chain: &Chain,
    resolved: &ResolvedChainAudioConfig,
) -> Vec<usize> {
    let regular_output_count: usize = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob.entries.len()),
            _ => None,
        })
        .sum();
    resolved
        .outputs
        .iter()
        .enumerate()
        .map(|(idx, out)| {
            let buf = crate::resolved_output_buffer_size_frames(out);
            let multiplier = if idx >= regular_output_count {
                ELASTIC_MULTIPLIER_INSERT_SEND
            } else {
                ELASTIC_MULTIPLIER_REGULAR
            };
            elastic_target_for_buffer(buf, multiplier)
        })
        .collect()
}
