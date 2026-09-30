//! Responsibility: finds a block list of a chain by its split path.
//!
//! #328: a chain's blocks live in the top level and in path A and path B of
//! each of its splits — at most a Mix, then a Y (spec §1.1). Every place that
//! turns "(index, path)" into a block goes through here, so an index inside a
//! path is never read against the top level, nor against another split.

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, PathRef, PathSide, SplitBlock};
use project::chain::Chain;

/// The top-level split `id` names: its position and the block itself.
pub(crate) fn split_by_id<'a>(chain: &'a Chain, id: &BlockId) -> Option<(usize, &'a SplitBlock)> {
    chain
        .blocks
        .iter()
        .enumerate()
        .find_map(|(index, block)| match &block.kind {
            AudioBlockKind::Split(split) if block.id == *id => Some((index, split)),
            _ => None,
        })
}

/// The list `path` names: the top level for `None`, else that lane of the
/// split it names. `None` when the path names a split this chain does not have.
pub(crate) fn list_at<'a>(chain: &'a Chain, path: Option<&PathRef>) -> Option<&'a [AudioBlock]> {
    let Some(path) = path else {
        return Some(&chain.blocks);
    };
    let (_, split) = split_by_id(chain, &path.split)?;
    Some(match &path.side {
        PathSide::A => &split.a,
        PathSide::B => &split.b,
    })
}

/// The block at `index` of the list `path` names.
pub(crate) fn block_at<'a>(
    chain: &'a Chain,
    index: usize,
    path: Option<&PathRef>,
) -> Option<&'a AudioBlock> {
    list_at(chain, path)?.get(index)
}

/// Where a block inserted "before `before`" lands in the list `path` names,
/// clamped to its end.
pub(crate) fn insert_index(chain: &Chain, before: usize, path: Option<&PathRef>) -> Option<usize> {
    list_at(chain, path).map(|list| before.min(list.len()))
}

/// The side as the Slint callbacks carry it: 0 = A, 1 = B.
pub(crate) fn side_index(side: &PathSide) -> i32 {
    match side {
        PathSide::A => 0,
        PathSide::B => 1,
    }
}

pub(crate) fn side_from_index(index: i32) -> Option<PathSide> {
    match index {
        0 => Some(PathSide::A),
        1 => Some(PathSide::B),
        _ => None,
    }
}

#[cfg(test)]
#[path = "chain_block_lists_tests.rs"]
mod tests;
