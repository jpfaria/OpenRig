//! Responsibility: decides whether a rebuilt block keeps its existing runtime node.
//!
//! Moved out of `runtime_block_builders.rs` unchanged (#328) so the chain
//! builder and the split builder draw old nodes through one rule.

use std::collections::HashMap;

use block_core::AudioChannelLayout;
use domain::ids::BlockId;

use crate::runtime_audio_frame::AudioProcessor;
use crate::runtime_state::{BlockRuntimeNode, FadeState, RuntimeProcessor};

/// The old nodes a rebuild may keep, keyed by block id.
pub(crate) fn reuse_pool(
    existing: Option<Vec<BlockRuntimeNode>>,
) -> HashMap<BlockId, BlockRuntimeNode> {
    existing
        .unwrap_or_default()
        .into_iter()
        .map(|node| (node.block_id.clone(), node))
        .collect::<HashMap<_, _>>()
}

/// The live node reused for `block`, or — when it cannot be — the node it
/// replaces (`None` if there is none), which a live edit hands over from.
pub(crate) fn try_reuse_block_node(
    reusable_nodes: &mut HashMap<BlockId, BlockRuntimeNode>,
    block: &project::block::AudioBlock,
    current_layout: AudioChannelLayout,
    content_mono: bool,
    sample_rate: f32,
    quiet: bool,
) -> Result<BlockRuntimeNode, Option<BlockRuntimeNode>> {
    let Some(mut node) = reusable_nodes.remove(&block.id) else {
        return Err(None);
    };
    if node.input_layout != current_layout {
        if !quiet {
            log::debug!(
                "[engine] cannot reuse block id={}: layout changed ({:?} → {:?})",
                block.id.0,
                node.input_layout,
                current_layout
            );
        }
        return Err(Some(node));
    }
    // Issue #588: the mono ↔ dual-mono decision depends on whether the
    // incoming signal is effectively mono. If that flipped (e.g. an upstream
    // block now produces stereo), the processor shape is wrong — rebuild.
    if node.content_mono != content_mono {
        return Err(Some(node));
    }
    // Exact match — reuse as-is
    if node.block_snapshot == *block {
        return Ok(node);
    }
    // Only enabled changed — reuse processor, update snapshot.
    // Exception: if the node is a Bypass (block was built while disabled and has no real
    // processor or stream_handle), enabling it requires a full rebuild.
    let mut snapshot_without_enabled = node.block_snapshot.clone();
    snapshot_without_enabled.enabled = block.enabled;
    if snapshot_without_enabled == *block {
        if matches!(node.processor, RuntimeProcessor::Bypass) && block.enabled {
            return Err(Some(node)); // force rebuild so we get a real processor + stream_handle
        }
        let was_disabled = !node.block_snapshot.enabled;
        node.block_snapshot = block.clone();
        // If block was just enabled, start a fade-in — warmed up first, its
        // processor sat frozen while the block was off (#987).
        if was_disabled && block.enabled {
            node.fade_state = FadeState::FadingIn {
                frames_remaining: crate::runtime_node_handover::WARMED_FADE_IN_FRAMES,
            };
        }
        return Ok(node);
    }
    // Issue #358 — params changed but kind/effect_type/model unchanged. Try to
    // retune the existing processor in place (preserves IIR state, smooths
    // coefficients), avoiding the click that a full rebuild produces. Only
    // mono / dual-mono variants are supported today; other variants fall
    // through to the rebuild path.
    if try_in_place_param_update(&mut node, block, sample_rate) {
        if !quiet {
            log::info!(
                "[engine] in-place param update for block id={} (no rebuild)",
                block.id.0
            );
        }
        node.block_snapshot = block.clone();
        return Ok(node);
    }
    if !quiet {
        log::info!(
            "[engine] cannot reuse block id={}: snapshot differs (params or kind changed)",
            block.id.0
        );
    }
    Err(Some(node))
}

/// Attempt to apply the new `block`'s params to `node`'s existing processor
/// without dropping it. Returns `true` only if the kind/effect_type/model are
/// unchanged AND the underlying processor accepts an in-place update.
///
/// Caller must update `node.block_snapshot` after a successful call so the
/// next reuse attempt sees the new params as the "current" state.
fn try_in_place_param_update(
    node: &mut BlockRuntimeNode,
    block: &project::block::AudioBlock,
    sample_rate: f32,
) -> bool {
    if !block.enabled || !node.block_snapshot.enabled {
        return false;
    }
    if std::mem::discriminant(&node.block_snapshot.kind) != std::mem::discriminant(&block.kind) {
        return false;
    }
    let (Some(prev), Some(next)) = (node.block_snapshot.model_ref(), block.model_ref()) else {
        return false;
    };
    if prev.effect_type != next.effect_type || prev.model != next.model {
        return false;
    }
    let RuntimeProcessor::Audio(audio) = &mut node.processor else {
        return false;
    };
    match audio {
        AudioProcessor::Mono(processor) => processor.try_in_place_update(next.params, sample_rate),
        AudioProcessor::DualMono { left, right } => {
            // Both channels share the same params — both must accept the update.
            // If either rejects, abort: the channels would otherwise diverge.
            let left_ok = left.try_in_place_update(next.params, sample_rate);
            let right_ok = right.try_in_place_update(next.params, sample_rate);
            left_ok && right_ok
        }
        // Stereo / StereoFromMono retune in place too. This is essential for
        // VST3 GUI plugins, which must NOT be re-instantiated on a param change
        // (a reload re-runs createInstance, which fails under the app's
        // NSApplication after the first instance — #251).
        AudioProcessor::Stereo(processor) | AudioProcessor::StereoFromMono(processor) => {
            processor.try_in_place_update(next.params, sample_rate)
        }
    }
}
