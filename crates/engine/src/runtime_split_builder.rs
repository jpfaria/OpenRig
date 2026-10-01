//! Responsibility: builds the runtime node a split block turns into.

use std::collections::HashMap;

use anyhow::Result;
use block_core::AudioChannelLayout;
use domain::ids::BlockId;
use project::block::split_params::{SPLIT_MODE, SPLIT_MODE_DUAL_MONO};
use project::block::{AudioBlock, SplitBlock, SplitEnd};
use project::chain::Chain;

use crate::runtime::FADE_IN_FRAMES;
use crate::runtime_audio_frame::ProcessorScratch;
use crate::runtime_block_builders::{
    build_nodes_for, bypass_runtime_node, next_block_instance_serial,
};
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_state::{BlockRuntimeNode, FadeState, RuntimeProcessor};

/// Build (or rebuild) the node of `block`, a split (#328). Every path is
/// built for a stereo bus — the split always hands them stereo frames — and
/// for mono content when the bus reaching the split is mono or Mode II feeds
/// each path one channel, so the #588 mono collapse still applies inside a
/// path. `reusable_nodes` may hold this split's previous node.
pub(crate) fn build_split_runtime_node(
    chain: &Chain,
    block: &AudioBlock,
    split: &SplitBlock,
    input_layout: AudioChannelLayout,
    content_mono: bool,
    sample_rate: f32,
    reusable_nodes: &mut HashMap<BlockId, BlockRuntimeNode>,
) -> Result<BlockRuntimeNode> {
    let mut previous = reusable_nodes.remove(&block.id).filter(|node| {
        node.input_layout == input_layout && matches!(node.processor, RuntimeProcessor::Split(_))
    });
    if previous.is_none() && !block.enabled {
        return Ok(bypass_runtime_node(block, input_layout, content_mono));
    }
    // #328: take the previous build's split state out of its node; its
    // delay lines continue in the new state below.
    let previous_state = previous.as_mut().and_then(|node| {
        match std::mem::replace(&mut node.processor, RuntimeProcessor::Bypass) {
            RuntimeProcessor::Split(state) => Some(state),
            _ => None,
        }
    });
    let path_content_mono =
        content_mono || split.params.get_string(SPLIT_MODE) == Some(SPLIT_MODE_DUAL_MONO);
    // #328: paths draw from the caller's pool, so a knob move keeps every
    // amp and a block dragged between paths keeps its processor.
    let mut paths = Vec::with_capacity(split.paths.len());
    for path in &split.paths {
        let blocks: Vec<&AudioBlock> = path.iter().collect();
        let (nodes, _, _) = build_nodes_for(
            chain,
            &blocks,
            AudioChannelLayout::Stereo,
            path_content_mono,
            sample_rate,
            reusable_nodes,
            None,
        )?;
        paths.push(nodes);
    }
    let mut state = SplitRuntimeState::new(
        matches!(split.end, SplitEnd::Mix),
        paths,
        SplitKnobs::from_params(&split.params, split.paths.len()),
        &block.id,
    );
    if let Some(previous_state) = previous_state {
        state.adopt_history(previous_state);
    }
    let (instance_serial, fade_state) = match previous {
        Some(node) => (
            node.instance_serial,
            split_fade(node.block_snapshot.enabled, block.enabled, node.fade_state),
        ),
        None => (
            next_block_instance_serial(),
            FadeState::FadingIn {
                frames_remaining: FADE_IN_FRAMES,
            },
        ),
    };
    Ok(BlockRuntimeNode {
        instance_serial,
        block_id: block.id.clone(),
        block_snapshot: block.clone(),
        input_layout,
        content_mono,
        output_layout: AudioChannelLayout::Stereo,
        scratch: ProcessorScratch::None,
        processor: RuntimeProcessor::Split(state),
        stream_handle: None,
        fade_state,
        fade_dry_buffer: Vec::new(),
        faulted: false,
        fault_reason: None,
        handover: None,
    })
}

/// A split that stays on or off keeps its fade; one switched on fades in and
/// one switched off fades out through its real paths.
fn split_fade(was_enabled: bool, enabled: bool, previous: FadeState) -> FadeState {
    match (was_enabled, enabled) {
        (false, true) => FadeState::FadingIn {
            frames_remaining: FADE_IN_FRAMES,
        },
        (true, false) => FadeState::FadingOut {
            frames_remaining: FADE_IN_FRAMES,
        },
        _ => previous,
    }
}
