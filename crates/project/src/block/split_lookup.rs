//! Responsibility: finds the splits among a chain's top-level blocks.
//!
//! #328 (spec §1.1): a chain may hold a Mix split and, after it, a Y split, so
//! no lookup here assumes the chain has a single split.

use super::split_block::{SplitBlock, SplitEnd};
use super::types::{AudioBlock, AudioBlockKind};

/// Every top-level split of `blocks` with its position, in chain order.
pub fn splits(blocks: &[AudioBlock]) -> impl Iterator<Item = (usize, &SplitBlock)> {
    blocks
        .iter()
        .enumerate()
        .filter_map(|(position, block)| match &block.kind {
            AudioBlockKind::Split(split) => Some((position, split)),
            _ => None,
        })
}

/// The first split of `blocks` and its position, if it has one.
pub fn find_split(blocks: &[AudioBlock]) -> Option<(usize, &SplitBlock)> {
    splits(blocks).next()
}

/// The first split of `blocks` that ends in `end`, and its position.
pub fn find_split_with_end(blocks: &[AudioBlock], end: SplitEnd) -> Option<(usize, &SplitBlock)> {
    splits(blocks).find(|(_, split)| split.end == end)
}

/// Whether `blocks` hold a Y split at any depth, so some path of the chain
/// ends in its own output nodes.
pub fn has_y_split(blocks: &[AudioBlock]) -> bool {
    splits(blocks).any(|(_, split)| {
        split.end == SplitEnd::Y || split.paths.iter().any(|path| has_y_split(path))
    })
}
