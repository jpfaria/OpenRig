//! Responsibility: turns a chain into the graph its row draws.
//!
//! #328 (spec §5.1, §5.2). The chain's `Split` becomes Part 5's `Parallel`
//! stage: lane A on top, lane B below; `Merge` ends it in the mixer node, `Fan`
//! (Y → A/B) ends each lane in its own output node. Stage 0 is the input node
//! and every top-level block is one stage — `graph_anchor` counts on that
//! numbering. Positions and "+" anchors come from Part 5
//! (`linear_chain_layout`, `insert_anchors`); this file only picks the stages
//! and the grid.

use project::block::{AudioBlock, AudioBlockKind, SplitEnd};
use project::chain::Chain;

use crate::chain_graph_ids::{
    INPUT_NODE_ID, OUTPUT_NODE_ID, PATH_A_OUTPUT_NODE_ID, PATH_B_OUTPUT_NODE_ID,
};
use crate::endpoint_checklist_items::IoLabels;
use crate::graph_view_model::{
    insert_anchors, linear_chain_layout, BlockBlueprint, ChainStage, GraphAnchor, GraphEdge,
    GraphNode, GridMetrics, NodeCategory, NodeKind, ParallelEnd,
};

/// Centre-to-centre: a 100 px card plus the strip's 32 px gap (`chain_row_blocks.slint:47`).
pub(crate) const COLUMN_SPACING: f32 = 132.0;
/// One strip row per lane (`chain_row.slint:100`: 108 px per row).
pub(crate) const LANE_SPACING: f32 = 108.0;
/// The first card's centre sits half a card in from the corner.
pub(crate) const CARD_HALF: f32 = 50.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChainGraph {
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
    pub(crate) anchors: Vec<GraphAnchor>,
    pub(crate) lanes: usize,
    pub(crate) columns: usize,
}

/// With two lanes the shared blocks sit between them, so the top lane's cards
/// still start at the row's top edge.
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
    let mut lanes_end_in_outputs = false;
    for block in &chain.blocks {
        let AudioBlockKind::Split(split) = &block.kind else {
            stages.push(ChainStage::Single(blueprint(block)));
            continue;
        };
        let mut a: Vec<BlockBlueprint> = split.a.iter().map(blueprint).collect();
        let mut b: Vec<BlockBlueprint> = split.b.iter().map(blueprint).collect();
        let end = match split.end {
            SplitEnd::Mix => ParallelEnd::Merge,
            SplitEnd::Y => {
                a.push(endpoint_node(
                    PATH_A_OUTPUT_NODE_ID,
                    &labels.path_a,
                    NodeCategory::Output,
                    NodeKind::IoOutput,
                ));
                b.push(endpoint_node(
                    PATH_B_OUTPUT_NODE_ID,
                    &labels.path_b,
                    NodeCategory::Output,
                    NodeKind::IoOutput,
                ));
                lanes_end_in_outputs = true;
                ParallelEnd::Fan
            }
        };
        stages.push(ChainStage::Parallel {
            lanes: vec![a, b],
            end,
        });
    }
    if !lanes_end_in_outputs {
        stages.push(ChainStage::Single(endpoint_node(
            OUTPUT_NODE_ID,
            &labels.output,
            NodeCategory::Output,
            NodeKind::IoOutput,
        )));
    }
    stages
}

pub(crate) fn chain_graph(chain: &Chain, labels: &IoLabels) -> ChainGraph {
    let lanes = if chain
        .blocks
        .iter()
        .any(|b| matches!(b.kind, AudioBlockKind::Split(_)))
    {
        2
    } else {
        1
    };
    let metrics = grid_metrics(lanes);
    let stages = chain_stages(chain, labels);
    let (nodes, edges) = linear_chain_layout(&stages, metrics);
    let anchors = insert_anchors(&stages, &nodes);
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

fn blueprint(block: &AudioBlock) -> BlockBlueprint {
    let label = match block.model_ref() {
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
