//! Responsibility: lists the Y leaves of a block list at any depth.
//!
//! #328 (spec §11.3): a leaf is a path of a Y split that holds no Y of its
//! own; each leaf ends in its own output node. A Y nested in a Mix path counts,
//! and a Y inside a Y path replaces that path with its own leaves.

use super::path_ref::PathRef;
use super::split_block::SplitEnd;
use super::types::{AudioBlock, AudioBlockKind};

/// Every Y leaf of `blocks`, depth first, in chain order.
pub fn y_leaves(blocks: &[AudioBlock]) -> Vec<PathRef> {
    let mut leaves = Vec::new();
    collect(blocks, &mut leaves);
    leaves
}

fn collect(blocks: &[AudioBlock], leaves: &mut Vec<PathRef>) {
    for block in blocks {
        let AudioBlockKind::Split(split) = &block.kind else {
            continue;
        };
        for (index, path) in split.paths.iter().enumerate() {
            let before = leaves.len();
            collect(path, leaves);
            if split.end == SplitEnd::Y && leaves.len() == before {
                leaves.push(PathRef {
                    split: block.id.clone(),
                    path: index,
                });
            }
        }
    }
}
