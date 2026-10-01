//! Responsibility: addresses blocks anywhere in a chain's split tree.
//!
//! #328 (spec §10.2): a split may sit inside another split's path, so every
//! address here walks the whole tree. Lookup by id lives in
//! `project::block::find_block_mut`; this module holds the structural
//! addressing the command layer needs on top of it.

use anyhow::{anyhow, Result};
use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, PathRef, PathSide, SplitBlock};

/// Take the block with `id` out of whichever list holds it.
pub fn remove_block(blocks: &mut Vec<AudioBlock>, id: &BlockId) -> Option<AudioBlock> {
    let (list, at) = list_holding(blocks, id)?;
    Some(list.remove(at))
}

/// The list that holds the block `id`, and the block's index in it.
pub fn list_holding<'a>(
    blocks: &'a mut Vec<AudioBlock>,
    id: &BlockId,
) -> Option<(&'a mut Vec<AudioBlock>, usize)> {
    if let Some(at) = blocks.iter().position(|b| b.id == *id) {
        return Some((blocks, at));
    }
    blocks.iter_mut().find_map(|block| match &mut block.kind {
        AudioBlockKind::Split(split) => {
            list_holding(&mut split.a, id).or_else(|| list_holding(&mut split.b, id))
        }
        _ => None,
    })
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

/// The split `id`, wherever it sits.
pub fn split_mut<'a>(blocks: &'a mut Vec<AudioBlock>, id: &BlockId) -> Result<&'a mut SplitBlock> {
    let (list, at) =
        list_holding(blocks, id).ok_or_else(|| anyhow!("split not found: {:?}", id))?;
    match &mut list[at].kind {
        AudioBlockKind::Split(split) => Ok(split),
        _ => Err(anyhow!("block {:?} is not a split", id)),
    }
}

fn lane_mut<'a>(
    blocks: &'a mut Vec<AudioBlock>,
    path: Option<&PathRef>,
) -> Result<&'a mut Vec<AudioBlock>> {
    let Some(path) = path else {
        return Ok(blocks);
    };
    let split = split_mut(blocks, &path.split)?;
    Ok(match path.side {
        PathSide::A => &mut split.a,
        PathSide::B => &mut split.b,
    })
}
