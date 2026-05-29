//! Issue #522: fast path for toggling a block's `enabled` flag without
//! rebuilding the chain runtime.
//!
//! The standard `update_chain_runtime_state` reuses existing block
//! processors when only `enabled` flips, but the GUI today goes through
//! the full `upsert_chain` path which also re-resolves CPAL devices and
//! re-validates I/O. None of that work is necessary for a per-block
//! boolean flip — `BlockRuntimeNode`'s `fade_state` already supports the
//! `FadingOut` / `FadingIn` transitions, the disabled processor stays
//! alive for instant re-enable, and the audio thread crossfades without
//! any allocation or lock contention.
//!
//! Extracted as its own module to keep `runtime_graph.rs` within the
//! 600-LOC file cap (it was already at the limit). Re-exported through
//! `runtime.rs` so callers keep using the `engine::runtime::*` path.
//!
//! Issue #580 follow-up: queue the toggle instead of taking
//! `processing.lock()`
//! ─────────────────────────────────────────────────────────────────
//!
//! The original implementation took `processing.lock()` BLOCKING on the
//! GUI thread and walked `input_states.blocks` in place. While that
//! looked cheap (a single in-place mutation), the audio thread's
//! `process_input_f32` acquires the same Mutex via `try_lock` and
//! returns early — emitting a silent buffer — whenever the try_lock
//! fails. With the OS preempting the GUI thread mid-toggle, that
//! produced an audible click on every UI block on/off at buffer = 32
//! (`audio_under_block_toggle_tests` reported 315 / 10 000 silenced
//! buffers under stress).
//!
//! The fix moves the mutation to the audio thread's own callback:
//!   - `set_block_enabled` (GUI) pushes `(BlockId, enabled)` to a
//!     lock-free `ArrayQueue` on the runtime. No `processing.lock()`,
//!     no contention with the audio thread.
//!   - `drain_pending_block_toggles` runs inside the audio thread's
//!     existing `try_lock`'d section in `process_input_f32`. It walks
//!     `input_states.blocks` and applies each queued toggle in place,
//!     transitioning `fade_state` so the existing click-safe crossfade
//!     handles the audible change.
//!
//! Failure modes that used to return `Err` synchronously now post to
//! the runtime's `error_queue` (the existing audio-side error channel
//! the UI already drains via `poll_errors`). The two existing failure
//! shapes are preserved:
//!   - re-enabling a block whose live node is a `RuntimeProcessor::Bypass`
//!     (needs a full rebuild — message starts with "needs full rebuild").
//!   - the block id is not present in any input runtime of the chain
//!     (caller bug — message starts with "not found").
//! The only synchronous error that survives is "the toggle queue is
//! full", which in practice means the audio thread has stalled and the
//! GUI is faster than the drain — a louder bug than a dropped toggle.

use anyhow::{anyhow, Result};

use domain::ids::BlockId;

use crate::runtime::FADE_IN_FRAMES;
use crate::runtime_state::{
    BlockError, ChainProcessingState, ChainRuntimeState, FadeState, RuntimeProcessor,
};

/// Queue a per-block enabled flip for the audio thread to apply on its
/// next callback. Returns `Err` only if the queue is full (the audio
/// thread is not draining — a stall louder than a dropped toggle).
///
/// The actual mutation runs on the audio thread inside
/// [`drain_pending_block_toggles`]. Per-block failures (Bypass needing
/// rebuild, block id not found) are posted to the runtime's
/// `error_queue` for the GUI to drain via `poll_errors`.
pub fn set_block_enabled(
    runtime: &ChainRuntimeState,
    block_id: &BlockId,
    enabled: bool,
) -> Result<()> {
    // A block whose live node is a `Bypass` (built while disabled, or
    // build-faulted) has no DSP to fade in. Queueing a re-enable would only
    // post "has no live processor" from the audio thread and leave the
    // block dead. Decline synchronously — wait-free read of the bypass
    // mirror, no `processing` lock — so the caller falls back to a full
    // rebuild that builds the real processor (issue #580 regression).
    // Disabling (`enabled == false`) never needs a processor, so it always
    // queues on the lock-free fast path.
    if enabled && runtime.bypass_block_ids.load().contains(block_id) {
        return Err(anyhow!(
            "block '{}' has no live processor — needs full rebuild to re-enable",
            block_id.0
        ));
    }
    runtime
        .pending_block_toggles
        .push((block_id.clone(), enabled))
        .map_err(|_| anyhow!("block-toggle queue full — audio thread stalled?"))
}

/// Drain every queued toggle and apply it in place. Called by the audio
/// thread from `process_input_f32`, inside the `processing` try_lock
/// guard — so the GUI thread's `set_block_enabled` never needs to take
/// the lock at all.
///
/// Returns the number of toggles applied (for tests / instrumentation).
pub(crate) fn drain_pending_block_toggles(
    runtime: &ChainRuntimeState,
    processing: &mut ChainProcessingState,
) -> usize {
    let mut applied = 0;
    while let Some((block_id, enabled)) = runtime.pending_block_toggles.pop() {
        apply_block_toggle(processing, &block_id, enabled, runtime);
        applied += 1;
    }
    applied
}

/// In-place mutation that flips `fade_state` for every node matching
/// `block_id` across every per-input runtime of the chain. Mirrors the
/// pre-#580 inline implementation but never takes the `processing`
/// lock itself (the audio-thread caller already holds it via
/// `process_input_f32`'s try_lock guard).
fn apply_block_toggle(
    processing: &mut ChainProcessingState,
    block_id: &BlockId,
    enabled: bool,
    runtime: &ChainRuntimeState,
) {
    let mut touched = 0usize;
    for input_state in processing.input_states.iter_mut() {
        for node in input_state.blocks.iter_mut() {
            if &node.block_snapshot.id != block_id {
                continue;
            }
            if enabled && matches!(node.processor, RuntimeProcessor::Bypass) {
                let _ = runtime.error_queue.push(BlockError {
                    block_id: block_id.clone(),
                    message: format!(
                        "block '{}' has no live processor — needs full rebuild to re-enable",
                        block_id.0
                    ),
                });
                continue;
            }
            let was_enabled = node.block_snapshot.enabled;
            if was_enabled != enabled {
                node.fade_state = if enabled {
                    FadeState::FadingIn {
                        frames_remaining: FADE_IN_FRAMES,
                    }
                } else {
                    FadeState::FadingOut {
                        frames_remaining: FADE_IN_FRAMES,
                    }
                };
            }
            node.block_snapshot.enabled = enabled;
            touched += 1;
        }
    }
    if touched == 0 {
        let _ = runtime.error_queue.push(BlockError {
            block_id: block_id.clone(),
            message: format!(
                "block '{}' not found in any input runtime of the chain",
                block_id.0
            ),
        });
    }
}
