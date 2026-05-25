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

use anyhow::{anyhow, Result};

use domain::ids::BlockId;

use crate::runtime::FADE_IN_FRAMES;
use crate::runtime_state::{lock_recover, ChainRuntimeState, FadeState, RuntimeProcessor};

/// Toggle the `enabled` flag of a block in a live chain runtime. Walks
/// every per-input `ChainProcessingState` of `runtime`, finds the
/// matching `BlockRuntimeNode` by id, and flips its `fade_state` so the
/// audio thread crossfades on the next callback. No chain re-resolve,
/// no processor rebuild, no CPAL queries.
///
/// Returns `Err` when:
/// - the block id is not found in any input runtime (caller can fall
///   back to a full `upsert_chain`); or
/// - re-enabling a block whose live node is a `RuntimeProcessor::Bypass`
///   (it has no real processor to fade in — needs a full rebuild).
pub fn set_block_enabled(
    runtime: &ChainRuntimeState,
    block_id: &BlockId,
    enabled: bool,
) -> Result<()> {
    let mut processing = lock_recover(&runtime.processing, "chain runtime");
    let mut touched = 0usize;
    for input_state in processing.input_states.iter_mut() {
        for node in input_state.blocks.iter_mut() {
            if &node.block_snapshot.id != block_id {
                continue;
            }
            if enabled && matches!(node.processor, RuntimeProcessor::Bypass) {
                return Err(anyhow!(
                    "block '{}' has no live processor — needs full rebuild to re-enable",
                    block_id.0
                ));
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
        return Err(anyhow!(
            "block '{}' not found in any input runtime of the chain",
            block_id.0
        ));
    }
    Ok(())
}
