//! Responsibility: addresses blocks inside a chain's split paths.
//!
//! #328: lookup lives in `project::block::find_block_mut`; this module holds
//! the structural addressing the command layer needs on top of it.

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind};

/// Take the block with `id` out of the top level or out of a split path.
pub fn remove_block(blocks: &mut Vec<AudioBlock>, id: &BlockId) -> Option<AudioBlock> {
    if let Some(at) = blocks.iter().position(|b| b.id == *id) {
        return Some(blocks.remove(at));
    }
    blocks.iter_mut().find_map(|block| match &mut block.kind {
        AudioBlockKind::Split(split) => take(&mut split.a, id).or_else(|| take(&mut split.b, id)),
        _ => None,
    })
}

fn take(lane: &mut Vec<AudioBlock>, id: &BlockId) -> Option<AudioBlock> {
    let at = lane.iter().position(|b| b.id == *id)?;
    Some(lane.remove(at))
}
