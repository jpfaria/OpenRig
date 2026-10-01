//! Responsibility: turns a graph anchor into a place in the chain.
//!
//! #328 (spec §5.1, §11). The graph reports a "+" or a drop by the anchor id
//! Part 5 gives each wire (`AnchorSlot::anchor_id`). `chain_graph_adapter`
//! lays a chain out as stage 0 = the input node, one stage per top-level block
//! (each split is one `Parallel` stage), then the output node — so top-level
//! stage `i` is position `i - 1`. Inside a split, lane `n` is path `n` and its
//! stages are that path's blocks one for one, at any depth. A place is a
//! position in one block list plus that list's path (`None` = top level).

use domain::ids::BlockId;
use project::block::PathRef;
use project::chain::Chain;

use crate::chain_block_lists::list_at;
use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::graph_view_model::AnchorSlot;

/// The slot an anchor id names — the inverse of `AnchorSlot::anchor_id`
/// (`"top:{i}"`, `"path:{split}:{path}:{i}"`). A split id may itself hold
/// colons, so the path and index are read from the right.
pub(crate) fn parse_anchor(anchor_id: &str) -> Option<AnchorSlot> {
    if let Some(index) = anchor_id.strip_prefix("top:") {
        return Some(AnchorSlot {
            path: None,
            index: index.parse().ok()?,
        });
    }
    let rest = anchor_id.strip_prefix("path:")?;
    let mut parts = rest.rsplitn(3, ':');
    let index = parts.next()?.parse().ok()?;
    let path = parts.next()?.parse().ok()?;
    let split = parts.next().filter(|s| !s.is_empty())?;
    Some(AnchorSlot {
        path: Some(PathRef {
            split: BlockId(split.to_string()),
            path,
        }),
        index,
    })
}

/// Where a new block goes: `position` in the list `path` names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InsertTarget {
    pub(crate) position: usize,
    pub(crate) path: Option<PathRef>,
}

/// The place an anchor stands for ("insert before" the slot's index).
pub(crate) fn insert_target(chain: &Chain, slot: &AnchorSlot) -> Option<InsertTarget> {
    match &slot.path {
        None => {
            // Stage 0 is the input node: nothing goes before it.
            let position = slot.index.checked_sub(1)?;
            (position <= chain.blocks.len()).then_some(InsertTarget {
                position,
                path: None,
            })
        }
        Some(path) => {
            let len = list_at(chain, Some(path))?.len();
            (slot.index <= len).then_some(InsertTarget {
                position: slot.index,
                path: Some(path.clone()),
            })
        }
    }
}

/// A drag: the block, its `MoveBlock.new_position` and the destination path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MoveTarget {
    pub(crate) block: BlockId,
    pub(crate) new_position: usize,
    pub(crate) path: Option<PathRef>,
}

pub(crate) fn move_target(chain: &Chain, node_id: &str, slot: &AnchorSlot) -> Option<MoveTarget> {
    let NodeRef::Block {
        id,
        path: from_path,
        index: from,
    } = resolve_node(chain, node_id)?
    else {
        return None;
    };
    let target = insert_target(chain, slot)?;
    if target.path != from_path {
        // Lifting it out of another list does not shift this one.
        return Some(MoveTarget {
            block: id,
            new_position: target.position,
            path: target.path,
        });
    }
    // Same list: the block is lifted out first, so everything to its right
    // shifts one slot left (the `block_reorder.rs:34-50` rule). Its own slot
    // and the gap right after it change nothing.
    if target.position == from || target.position == from + 1 {
        return None;
    }
    let new_position = if target.position > from {
        target.position - 1
    } else {
        target.position
    };
    Some(MoveTarget {
        block: id,
        new_position,
        path: target.path,
    })
}

#[cfg(test)]
#[path = "graph_anchor_tests.rs"]
mod tests;
