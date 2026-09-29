//! Responsibility: visits every block of a list through the paths of its split.
//!
//! #328 (spec §1.4): every model walker — lookup, descriptors, scenes, edit
//! capture, model swap, load-time disable — reaches the blocks inside a split's
//! paths through these helpers, so the recursion lives in one place. Signal
//! order: a split, then its path A, then its path B. Select options are not
//! visited: a select is one slot whose inactive options are not in the signal
//! path.

use super::types::{AudioBlock, AudioBlockKind};

/// Every block of `blocks`, paths included, in signal order.
pub fn walk_blocks(blocks: &[AudioBlock]) -> Vec<&AudioBlock> {
    let mut walked = Vec::with_capacity(blocks.len());
    push_walk(blocks, &mut walked);
    walked
}

fn push_walk<'a>(blocks: &'a [AudioBlock], walked: &mut Vec<&'a AudioBlock>) {
    for block in blocks {
        walked.push(block);
        if let AudioBlockKind::Split(split) = &block.kind {
            push_walk(&split.a, walked);
            push_walk(&split.b, walked);
        }
    }
}

/// Mutable twin of [`walk_blocks`]: `f` runs on every block, on a split before
/// its paths.
pub fn for_each_block_mut(blocks: &mut [AudioBlock], f: &mut dyn FnMut(&mut AudioBlock)) {
    for block in blocks {
        f(block);
        if let AudioBlockKind::Split(split) = &mut block.kind {
            for_each_block_mut(&mut split.a, f);
            for_each_block_mut(&mut split.b, f);
        }
    }
}

/// The block with `id` anywhere in `blocks`, paths included.
pub fn find_block_mut<'a>(blocks: &'a mut [AudioBlock], id: &str) -> Option<&'a mut AudioBlock> {
    blocks
        .iter_mut()
        .find_map(|block| find_in_block_mut(block, id))
}

fn find_in_block_mut<'a>(block: &'a mut AudioBlock, id: &str) -> Option<&'a mut AudioBlock> {
    if block.id.0 == id {
        return Some(block);
    }
    match &mut block.kind {
        AudioBlockKind::Split(split) => {
            find_block_mut(&mut split.a, id).or_else(|| find_block_mut(&mut split.b, id))
        }
        _ => None,
    }
}
