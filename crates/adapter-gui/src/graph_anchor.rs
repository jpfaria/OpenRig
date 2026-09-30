//! Responsibility: turns a graph anchor into a place in the chain.
//!
//! #328 (spec §5.1). The graph reports a "+" or a drop by the anchor id Part 5
//! gives each wire (`AnchorSlot::anchor_id`). `chain_graph_adapter` lays a
//! chain out as stage 0 = the input node, one stage per top-level block (each
//! split is one `Parallel` stage), then the output node — so stage `i` is
//! top-level position `i - 1`, and lane 0 / 1 of a split's stage is path A / B
//! of that split. A place is a position in one block list plus that list's
//! path (`None` = top level, spec §3).

use domain::ids::BlockId;
use project::block::{AudioBlockKind, PathRef};
use project::chain::Chain;

use crate::chain_block_lists::{list_at, side_from_index};
use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::graph_view_model::AnchorSlot;

/// The slot an anchor id names — the inverse of `AnchorSlot::anchor_id`
/// (`"stage:{i}"`, `"lane:{stage}:{lane}:{i}"`).
pub(crate) fn parse_anchor(anchor_id: &str) -> Option<AnchorSlot> {
    let mut parts = anchor_id.split(':');
    let slot = match parts.next()? {
        "stage" => AnchorSlot::Stage {
            index: parts.next()?.parse().ok()?,
        },
        "lane" => AnchorSlot::Lane {
            stage: parts.next()?.parse().ok()?,
            lane: parts.next()?.parse().ok()?,
            index: parts.next()?.parse().ok()?,
        },
        _ => return None,
    };
    parts.next().is_none().then_some(slot)
}

/// Where a new block goes: `position` in the list `path` names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InsertTarget {
    pub(crate) position: usize,
    pub(crate) path: Option<PathRef>,
}

/// The place an anchor stands for ("insert before" the slot's index).
pub(crate) fn insert_target(chain: &Chain, slot: &AnchorSlot) -> Option<InsertTarget> {
    match *slot {
        AnchorSlot::Stage { index } => {
            // Stage 0 is the input node: nothing goes before it.
            let position = index.checked_sub(1)?;
            (position <= chain.blocks.len()).then_some(InsertTarget {
                position,
                path: None,
            })
        }
        AnchorSlot::Lane { stage, lane, index } => {
            let block = chain.blocks.get(stage.checked_sub(1)?)?;
            let AudioBlockKind::Split(_) = &block.kind else {
                return None;
            };
            let path = PathRef {
                split: block.id.clone(),
                side: side_from_index(i32::try_from(lane).ok()?)?,
            };
            let len = list_at(chain, Some(&path))?.len();
            (index <= len).then_some(InsertTarget {
                position: index,
                path: Some(path),
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
