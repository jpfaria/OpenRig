//! Responsibility: places the insert anchors on a laid-out chain's wires
//!
//! One anchor per wire, at the wire's midpoint — exactly where the
//! horizontal-S Bézier the canvas draws crosses t = 0.5. Each anchor
//! carries the slot a block added or dropped there lands in (#328 §5.1).

use domain::ids::BlockId;
use project::block::PathRef;

use super::routing_ids::{merge_node_id, split_node_id};
use super::types::{ChainStage, GraphNode, NodeKind, ParallelEnd};

/// Where a block added or dropped on an anchor lands: "insert before stage
/// `index`" of the list `path` names (`None` = the top-level stage list),
/// in the ORIGINAL list, before any move (#328 §11: any depth).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorSlot {
    pub path: Option<PathRef>,
    pub index: usize,
}

impl AnchorSlot {
    /// Stable id of the anchor at this slot — unique within one graph:
    /// `"top:{index}"` or `"path:{split}:{path}:{index}"`.
    pub fn anchor_id(&self) -> String {
        match &self.path {
            None => format!("top:{}", self.index),
            Some(path) => format!("path:{}:{}:{}", path.split.0, path.path, self.index),
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
    walk(stages, None, None, nodes, &mut anchors);
    anchors
}

/// Anchors of the stage list `path` names, fed from `tail`; returns the
/// list's tail, `None` once it fanned out.
fn walk(
    stages: &[ChainStage],
    path: Option<&PathRef>,
    mut tail: Option<String>,
    nodes: &[GraphNode],
    anchors: &mut Vec<GraphAnchor>,
) -> Option<String> {
    for (index, stage) in stages.iter().enumerate() {
        let slot = AnchorSlot {
            path: path.cloned(),
            index,
        };
        match stage {
            ChainStage::Single(block) => {
                if let Some(prev) = tail.take() {
                    push_anchor(anchors, nodes, &prev, &block.id, slot);
                }
                tail = Some(block.id.clone());
            }
            ChainStage::Parallel { lanes, .. } if lanes.is_empty() => {}
            ChainStage::Parallel {
                split_id,
                lanes,
                end,
            } => {
                let split_node = split_node_id(split_id);
                let merge_node = merge_node_id(split_id);
                if let Some(prev) = tail.take() {
                    push_anchor(anchors, nodes, &prev, &split_node, slot);
                }
                for (lane, lane_stages) in lanes.iter().enumerate() {
                    let lane_path = PathRef {
                        split: BlockId(split_id.clone()),
                        path: lane,
                    };
                    let lane_tail = walk(
                        lane_stages,
                        Some(&lane_path),
                        Some(split_node.clone()),
                        nodes,
                        anchors,
                    );
                    if let (ParallelEnd::Merge, Some(from)) = (end, lane_tail) {
                        let slot = AnchorSlot {
                            path: Some(lane_path),
                            index: lane_stages.len(),
                        };
                        push_anchor(anchors, nodes, &from, &merge_node, slot);
                    }
                }
                tail = match end {
                    ParallelEnd::Merge => Some(merge_node),
                    ParallelEnd::Fan => None,
                };
            }
        }
    }
    tail
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
