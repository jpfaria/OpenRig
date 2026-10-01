//! Responsibility: resolves the bus layout a split hands to the block after it.
//!
//! #328 (spec §3): each path is walked from the layout the split feeds it —
//! Mode I (`same`) passes the incoming bus, Mode II (`dual_mono`) feeds each
//! path a dual-mono stereo bus — and every enabled block in it must accept
//! what it gets. The mixer (Split → Mix) and each Y output take either layout
//! from a path and emit the stereo bus (invariant #5).

use anyhow::{anyhow, Result};
use block_core::AudioChannelLayout;
use project::block::split_block::SplitBlock;
use project::block::split_params::{SPLIT_MODE, SPLIT_MODE_DUAL_MONO};
use project::block::{path_letter, AudioBlock};
use project::chain::Chain;

use super::resolve_block_output_layout;

pub(super) fn resolve_split_output_layout(
    chain: &Chain,
    block: &AudioBlock,
    split: &SplitBlock,
    input_layout: AudioChannelLayout,
) -> Result<AudioChannelLayout> {
    let path_input = if split.params.get_string(SPLIT_MODE) == Some(SPLIT_MODE_DUAL_MONO) {
        AudioChannelLayout::Stereo
    } else {
        input_layout
    };
    for (index, lane) in split.paths.iter().enumerate() {
        let side = path_letter(index);
        let mut layout = path_input;
        for path_block in lane.iter().filter(|b| b.enabled) {
            layout = resolve_block_output_layout(chain, path_block, layout)
                .map_err(|e| anyhow!("split '{}' path {side}: {e}", block.id.0))?;
        }
    }
    // The split's own knobs last, so a bad block inside a path is reported
    // with its path even when `validate_params` also walks the paths.
    block
        .validate_params()
        .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))?;
    Ok(AudioChannelLayout::Stereo)
}
