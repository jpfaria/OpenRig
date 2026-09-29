//! Responsibility: addresses blocks inside a chain's split paths.
//!
//! #328: lookup lives in `project::block::find_block_mut`; this module holds
//! the structural addressing the command layer needs on top of it.

use anyhow::{anyhow, Result};
use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, PathRef, PathSide};

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

/// Put `block` at `position` (clamped to the end) of the top level when
/// `path` is `None`, else of the named split path.
pub fn insert_block(
    blocks: &mut Vec<AudioBlock>,
    path: Option<&PathRef>,
    position: usize,
    block: AudioBlock,
) -> Result<()> {
    let lane = lane_mut(blocks, path)?;
    let at = position.min(lane.len());
    lane.insert(at, block);
    Ok(())
}

fn lane_mut<'a>(
    blocks: &'a mut Vec<AudioBlock>,
    path: Option<&PathRef>,
) -> Result<&'a mut Vec<AudioBlock>> {
    let Some(path) = path else {
        return Ok(blocks);
    };
    let target = blocks
        .iter_mut()
        .find(|b| b.id == path.split)
        .ok_or_else(|| anyhow!("split not found: {:?}", path.split))?;
    match &mut target.kind {
        AudioBlockKind::Split(split) => Ok(match path.side {
            PathSide::A => &mut split.a,
            PathSide::B => &mut split.b,
        }),
        _ => Err(anyhow!("block {:?} is not a split", path.split)),
    }
}
