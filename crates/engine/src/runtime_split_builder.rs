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

/// Build (or rebuild) the node of `block`, a split (#328). Both paths are
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
    let previous = reusable_nodes.remove(&block.id).filter(|node| {
        node.input_layout == input_layout && matches!(node.processor, RuntimeProcessor::Split(_))
    });
    if previous.is_none() && !block.enabled {
        return Ok(bypass_runtime_node(block, input_layout, content_mono));
    }
    let path_content_mono =
        content_mono || split.params.get_string(SPLIT_MODE) == Some(SPLIT_MODE_DUAL_MONO);
    let a_blocks: Vec<&AudioBlock> = split.a.iter().collect();
    let b_blocks: Vec<&AudioBlock> = split.b.iter().collect();
    let mut path_pool: HashMap<BlockId, BlockRuntimeNode> = HashMap::new();
    let (a, _, _) = build_nodes_for(
        chain,
        &a_blocks,
        AudioChannelLayout::Stereo,
        path_content_mono,
        sample_rate,
        &mut path_pool,
        None,
    )?;
    let (b, _, _) = build_nodes_for(
        chain,
        &b_blocks,
        AudioChannelLayout::Stereo,
        path_content_mono,
        sample_rate,
        &mut path_pool,
        None,
    )?;
    let state = SplitRuntimeState::new(
        matches!(split.end, SplitEnd::Mix),
        a,
        b,
        SplitKnobs::from_params(&split.params),
        &block.id,
    );
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
