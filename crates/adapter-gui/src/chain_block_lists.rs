//! Responsibility: finds a block list of a chain by its split path.
//!
//! #328: a chain's blocks live in up to three lists — the top level and, when
//! the chain has a split, its path A and path B (spec §1.1). Every place that
//! turns "(index, path)" into a block goes through here, so an index inside a
//! path is never read against the top level.

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, PathRef, PathSide, SplitBlock};
use project::chain::Chain;

/// The chain's split: its top-level position, its id and the block itself.
pub(crate) fn split_of(chain: &Chain) -> Option<(usize, &BlockId, &SplitBlock)> {
    chain
        .blocks
        .iter()
        .enumerate()
        .find_map(|(index, block)| match &block.kind {
            AudioBlockKind::Split(split) => Some((index, &block.id, split)),
            _ => None,
        })
}

/// The list `path` names: the top level for `None`, else that lane of the
/// chain's split. `None` when the path names a split this chain does not have.
pub(crate) fn list_at<'a>(chain: &'a Chain, path: Option<&PathRef>) -> Option<&'a [AudioBlock]> {
    let Some(path) = path else {
        return Some(&chain.blocks);
    };
    let (_, id, split) = split_of(chain)?;
    if *id != path.split {
        return None;
    }
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
