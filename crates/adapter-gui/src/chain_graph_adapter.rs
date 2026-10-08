//! Responsibility: turns a chain into the graph its row draws.
//!
//! #328 (spec §5.1, §5.2, §11). Every split, at any depth, becomes one
//! `Parallel` stage with one lane per path, top to bottom; `Merge` ends it in
//! the mixer node, `Fan` (a Y) ends each leaf path in its own output node.
//! Stage 0 is the input node and every top-level block is one stage —
//! `graph_anchor` counts on that numbering. Positions and "+" anchors come
//! from `linear_chain_layout` and `insert_anchors`; this file only picks the
//! stages and the grid.

use project::block::{has_y_split, AudioBlock, AudioBlockKind, PathRef, SplitEnd};
use project::chain::Chain;

use crate::chain_graph_ids::{leaf_output_node_id, INPUT_NODE_ID, OUTPUT_NODE_ID};
use crate::endpoint_checklist_items::IoLabels;
use crate::graph_view_model::{
    insert_anchors, linear_chain_layout, stage_extent, BlockBlueprint, ChainStage, GraphAnchor,
    GraphEdge, GraphNode, GridMetrics, NodeCategory, NodeKind, ParallelEnd,
};

/// Centre-to-centre: a 100 px card plus the strip's 32 px gap (`chain_row_blocks.slint:47`).
pub(crate) const COLUMN_SPACING: f32 = 132.0;
/// One lane holds the gear and its two-line caption (`chain_row.slint`
/// `lane-pitch`: 150 px per lane, #398).
pub(crate) const LANE_SPACING: f32 = 150.0;
/// The first card's centre sits half a card in from the corner.
pub(crate) const CARD_HALF: f32 = 50.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChainGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
    pub(crate) anchors: Vec<GraphAnchor>,
    /// Rows of the grid: one per lane at the deepest fan-out.
    pub(crate) lanes: usize,
    pub(crate) columns: usize,
}

/// With several rows the shared blocks sit on their centre, so the top row's
/// cards still start at the row's top edge.
pub(crate) fn grid_metrics(lanes: usize) -> GridMetrics {
    GridMetrics {
        column_spacing: COLUMN_SPACING,
        lane_spacing: LANE_SPACING,
        origin_x: CARD_HALF,
        origin_y: CARD_HALF + (lanes.max(1) - 1) as f32 * LANE_SPACING / 2.0,
    }
}

pub(crate) fn chain_stages(chain: &Chain, labels: &IoLabels) -> Vec<ChainStage> {
    let mut stages = vec![ChainStage::Single(endpoint_node(
        INPUT_NODE_ID,
        &labels.input,
        NodeCategory::Input,
        NodeKind::IoInput,
    ))];
    stages.extend(list_stages(&chain.blocks, labels));
    if !has_y_split(&chain.blocks) {
        stages.push(ChainStage::Single(endpoint_node(
            OUTPUT_NODE_ID,
            &labels.output,
            NodeCategory::Output,
            NodeKind::IoOutput,
        )));
    }
    stages
}

/// One stage per block of `blocks`; a split nests its paths as lanes.
fn list_stages(blocks: &[AudioBlock], labels: &IoLabels) -> Vec<ChainStage> {
    blocks
        .iter()
        .map(|block| match &block.kind {
            AudioBlockKind::Split(split) => {
                let lanes = split
                    .paths
                    .iter()
                    .enumerate()
                    .map(|(at, path)| {
                        let mut lane = list_stages(path, labels);
                        if split.end == SplitEnd::Y && !has_y_split(path) {
                            let leaf = PathRef {
                                split: block.id.clone(),
                                path: at,
                            };
                            lane.push(ChainStage::Single(endpoint_node(
                                &leaf_output_node_id(&leaf),
                                labels.leaf(&leaf),
                                NodeCategory::Output,
                                NodeKind::IoOutput,
                            )));
                        }
                        lane
                    })
                    .collect();
                let end = match split.end {
                    SplitEnd::Mix => ParallelEnd::Merge,
                    SplitEnd::Y => ParallelEnd::Fan,
                };
                ChainStage::Parallel {
                    split_id: block.id.0.clone(),
                    lanes,
                    end,
                }
            }
            _ => ChainStage::Single(blueprint(block, labels)),
        })
        .collect()
}

pub(crate) fn chain_graph(chain: &Chain, labels: &IoLabels) -> ChainGraph {
    let stages = chain_stages(chain, labels);
    let lanes = stage_extent(&stages).1;
    let metrics = grid_metrics(lanes);
    let (nodes, edges) = linear_chain_layout(&stages, metrics);
    let anchors = insert_anchors(&stages, &nodes, &edges);
    let columns = nodes
        .iter()
        .map(|n| ((n.x - metrics.origin_x) / metrics.column_spacing).round() as usize + 1)
        .max()
        .unwrap_or(1);
    ChainGraph {
        nodes,
        edges,
        anchors,
        lanes,
        columns,
    }
}

/// A port block (insert, mid-chain input/output) is named after what it plays
/// through (`port_block_names`).
fn blueprint(block: &AudioBlock, labels: &IoLabels) -> BlockBlueprint {
    let port = labels.port(&block.id.0);
    let label = match block.model_ref() {
        _ if !port.is_empty() => port.to_string(),
        Some(model) => project::catalog::model_display_name(model.effect_type, model.model),
        None => block.kind.label().to_uppercase(),
    };
    let mut node = BlockBlueprint::new(block.id.0.clone(), label, NodeCategory::Other);
    node.bypass = !block.enabled;
    node
}

fn endpoint_node(id: &str, label: &str, category: NodeCategory, kind: NodeKind) -> BlockBlueprint {
    BlockBlueprint::new(id, label, category).with_kind(kind)
}

#[cfg(test)]
#[path = "chain_graph_adapter_tests.rs"]
mod tests;
