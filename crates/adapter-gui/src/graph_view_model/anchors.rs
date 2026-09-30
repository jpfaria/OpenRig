//! Responsibility: places the insert anchors on a laid-out chain's wires
//!
//! One anchor per wire, at the wire's midpoint — exactly where the
//! horizontal-S Bézier the canvas draws crosses t = 0.5. Each anchor
//! carries the slot a block added or dropped there lands in (#328 §5.1).

use super::routing_ids::{merge_node_id, split_node_id};
use super::types::{ChainStage, GraphNode, NodeKind, ParallelEnd};

/// Where a block added or dropped on an anchor lands. Indices are in the
/// ORIGINAL stage list / lane, before any move: "insert before this one".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSlot {
    /// Before top-level stage `index`.
    Stage { index: usize },
    /// Before blueprint `index` of lane `lane` in the parallel stage `stage`.
    Lane {
        stage: usize,
        lane: usize,
        index: usize,
    },
}

impl AnchorSlot {
    /// Stable id of the anchor at this slot — unique within one graph.
    pub fn anchor_id(self) -> String {
        match self {
            Self::Stage { index } => format!("stage:{index}"),
            Self::Lane { stage, lane, index } => format!("lane:{stage}:{lane}:{index}"),
        }
    }
}

/// A "+" and drop target on one wire of the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphAnchor {
    /// [`AnchorSlot::anchor_id`] of `slot`.
    pub id: String,
    /// The wire this anchor sits on.
    pub from_id: String,
    pub to_id: String,
    pub slot: AnchorSlot,
    /// Layout-space midpoint of the wire.
    pub x: f32,
    pub y: f32,
    /// An empty segment — no block at either end of the wire — keeps its
    /// "+" visible, like the chain row's empty insert slot.
    pub always_visible: bool,
}

/// Anchors for the graph `linear_chain_layout(stages, ..)` returned as
/// `nodes`. The walk mirrors the chain builder's; `every_wire_gets_exactly_
/// one_anchor` pins that they agree. A wire whose ends are missing from
/// `nodes` gets no anchor (panic-free).
pub fn insert_anchors(stages: &[ChainStage], nodes: &[GraphNode]) -> Vec<GraphAnchor> {
    let mut anchors = Vec::new();
    let mut prev_tail: Option<String> = None;
    let mut split_counter: usize = 0;

    for (index, stage) in stages.iter().enumerate() {
        match stage {
            ChainStage::Single(block) => {
                if let Some(prev) = prev_tail.take() {
                    let slot = AnchorSlot::Stage { index };
                    push_anchor(&mut anchors, nodes, &prev, &block.id, slot);
                }
                prev_tail = Some(block.id.clone());
            }
            ChainStage::Parallel { lanes, .. } if lanes.is_empty() => {}
            ChainStage::Parallel { lanes, end } => {
                split_counter += 1;
                let split_id = split_node_id(split_counter);
                let merge_id = merge_node_id(split_counter);
                if let Some(prev) = prev_tail.take() {
                    let slot = AnchorSlot::Stage { index };
                    push_anchor(&mut anchors, nodes, &prev, &split_id, slot);
                }
                for (lane, blueprints) in lanes.iter().enumerate() {
                    let mut from = split_id.clone();
                    for (at, block) in blueprints.iter().enumerate() {
                        let slot = AnchorSlot::Lane {
                            stage: index,
                            lane,
                            index: at,
                        };
                        push_anchor(&mut anchors, nodes, &from, &block.id, slot);
                        from = block.id.clone();
                    }
                    if *end == ParallelEnd::Merge {
                        let slot = AnchorSlot::Lane {
                            stage: index,
                            lane,
                            index: blueprints.len(),
                        };
                        push_anchor(&mut anchors, nodes, &from, &merge_id, slot);
                    }
                }
                prev_tail = match end {
                    ParallelEnd::Merge => Some(merge_id),
                    ParallelEnd::Fan => None,
                };
            }
        }
    }

    anchors
}

fn push_anchor(
    anchors: &mut Vec<GraphAnchor>,
    nodes: &[GraphNode],
    from: &str,
    to: &str,
    slot: AnchorSlot,
) {
    let find = |id: &str| nodes.iter().find(|n| n.id == id);
    let (Some(a), Some(b)) = (find(from), find(to)) else {
        return;
    };
    anchors.push(GraphAnchor {
        id: slot.anchor_id(),
        from_id: from.to_string(),
        to_id: to.to_string(),
        slot,
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
        always_visible: a.kind != NodeKind::Block && b.kind != NodeKind::Block,
    });
}
