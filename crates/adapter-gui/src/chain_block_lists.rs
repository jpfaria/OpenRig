//! Responsibility: finds a block list of a chain by its split path.
//!
//! #328 (spec §11): a chain's blocks live in the top level and in the paths
//! of its splits, nested to any depth. Every place that turns
//! "(index, path)" into a block goes through here, so an index inside a path
//! is never read against the top level, nor against another split.

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, PathRef, SplitBlock};
use project::chain::Chain;

/// The split `id` names, at any depth: its position in the list that holds
/// it and the block itself.
pub(crate) fn split_by_id<'a>(chain: &'a Chain, id: &BlockId) -> Option<(usize, &'a SplitBlock)> {
    split_in(&chain.blocks, id)
}

fn split_in<'a>(blocks: &'a [AudioBlock], id: &BlockId) -> Option<(usize, &'a SplitBlock)> {
    blocks
        .iter()
        .enumerate()
        .find_map(|(index, block)| match &block.kind {
            AudioBlockKind::Split(split) if block.id == *id => Some((index, split)),
            AudioBlockKind::Split(split) => split.paths.iter().find_map(|path| split_in(path, id)),
            _ => None,
        })
}

/// The list `path` names: the top level for `None`, else that path of the
/// split it names. `None` when the path names a split or a path this chain
/// does not have.
pub(crate) fn list_at<'a>(chain: &'a Chain, path: Option<&PathRef>) -> Option<&'a [AudioBlock]> {
    let Some(path) = path else {
        return Some(&chain.blocks);
    };
    let (_, split) = split_by_id(chain, &path.split)?;
    split.paths.get(path.path).map(Vec::as_slice)
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

/// The path index as the Slint callbacks carry it.
pub(crate) fn path_index(path: &PathRef) -> i32 {
    i32::try_from(path.path).unwrap_or(i32::MAX)
}

/// The path index a Slint callback carried, `None` when negative.
pub(crate) fn path_from_index(index: i32) -> Option<usize> {
    usize::try_from(index).ok()
}

#[cfg(test)]
#[path = "chain_block_lists_tests.rs"]
mod tests;
