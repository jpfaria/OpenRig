//! Responsibility: builds a positioned graph from a chain's stages
//!
//! Layout strategy:
//!
//! - [`super::types::ChainStage::Single`] blocks sit on the central lane and
//!   advance the column cursor by one.
//! - `Parallel` starts with an auto-generated split node and places each
//!   lane on its own row (above/below the centre, distributed
//!   symmetrically). With [`ParallelEnd::Merge`] the lanes meet again at an
//!   auto-generated merge node on the column after the longest lane, and
//!   the next stage continues from it. With [`ParallelEnd::Fan`] there is
//!   no merge node: each lane's last blueprint is its terminal, and the
//!   terminals line up on the longest lane's last column (#328).

use super::types::{
    BlockBlueprint, ChainStage, GraphEdge, GraphNode, GridMetrics, NodeCategory, ParallelEnd,
};

/// Build a positioned graph from a sequence of [`ChainStage`]s.
///
/// Returns the (nodes, edges) pair ready to push to the Slint side. IDs
/// must be unique across the whole input — duplicates produce undefined
/// behaviour at the UI level (the panic-free contract is kept here, but
/// the UI may render only one of the duplicates). A stage after a
/// [`ParallelEnd::Fan`] has nothing to connect from and is left
/// unconnected; `validate_stages` reports it.
pub fn linear_chain_layout(
    stages: &[ChainStage],
    metrics: GridMetrics,
) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut col: usize = 0;
    let mut prev_tail: Option<String> = None;
    let mut split_counter: usize = 0;

    for stage in stages {
        match stage {
            ChainStage::Single(block) => {
                let node = position_block(block, col, 0, &metrics);
                if let Some(prev) = prev_tail.take() {
                    edges.push(GraphEdge {
                        from_id: prev,
                        to_id: node.id.clone(),
                    });
                }
                prev_tail = Some(node.id.clone());
                nodes.push(node);
                col += 1;
            }
            ChainStage::Parallel { lanes, .. } if lanes.is_empty() => {
                // No-op — nothing to render, no column consumed.
            }
            ChainStage::Parallel { lanes, end } => {
                split_counter += 1;
                let split_id = format!("__split_{split_counter}");
                let merge_id = format!("__merge_{split_counter}");

                let longest = lanes.iter().map(Vec::len).max().unwrap_or(0);
                let split_col = col;

                // Split node sits at split_col on the centre lane.
                nodes.push(routing_node(&split_id, split_col, &metrics));
                if let Some(prev) = prev_tail.take() {
                    edges.push(GraphEdge {
                        from_id: prev,
                        to_id: split_id.clone(),
                    });
                }

                // Each lane occupies its own row. With N lanes, rows are
                // -N/2..N/2 around the centre; 2 lanes → -0.5 / +0.5.
                let n_lanes = lanes.len() as f32;
                for (lane_idx, lane) in lanes.iter().enumerate() {
                    let lane_offset = lane_idx as f32 - (n_lanes - 1.0) / 2.0;
                    let mut last_in_lane = split_id.clone();
                    for (block_idx, block) in lane.iter().enumerate() {
                        let column = lane_column(*end, split_col, block_idx, lane.len(), longest);
                        let node = position_block_lane(block, column, lane_offset, &metrics);
                        edges.push(GraphEdge {
                            from_id: last_in_lane,
                            to_id: node.id.clone(),
                        });
                        last_in_lane = node.id.clone();
                        nodes.push(node);
                    }
                    if *end == ParallelEnd::Merge {
                        edges.push(GraphEdge {
                            from_id: last_in_lane,
                            to_id: merge_id.clone(),
                        });
                    }
                }

                match end {
                    ParallelEnd::Merge => {
                        // Merge node sits on the column after the longest
                        // lane, on the centre lane.
                        let merge_col = split_col + longest + 1;
                        nodes.push(routing_node(&merge_id, merge_col, &metrics));
                        prev_tail = Some(merge_id);
                        col = merge_col + 1;
                    }
                    ParallelEnd::Fan => {
                        // Every lane already ended at its own terminal.
                        prev_tail = None;
                        col = split_col + longest + 1;
                    }
                }
            }
        }
    }

    (nodes, edges)
}

/// Column of blueprint `index` in a lane of `len` blueprints. In a Fan the
/// lane's last blueprint is its terminal and lines up with every other
/// lane's terminal on the longest lane's last column, the way a Y chain's
/// output nodes sit side by side.
fn lane_column(
    end: ParallelEnd,
    split_col: usize,
    index: usize,
    len: usize,
    longest: usize,
) -> usize {
    if end == ParallelEnd::Fan && index + 1 == len {
        split_col + longest
    } else {
        split_col + 1 + index
    }
}

/// An auto-generated split or merge node: no label, `Util` category, on
/// the centre lane.
fn routing_node(id: &str, col: usize, metrics: &GridMetrics) -> GraphNode {
    GraphNode {
        id: id.to_string(),
        label: String::new(),
        category: NodeCategory::Util,
        x: metrics.origin_x + col as f32 * metrics.column_spacing,
        y: metrics.origin_y,
        bypass: false,
    }
}

fn position_block(
    block: &BlockBlueprint,
    col: usize,
    lane: i32,
    metrics: &GridMetrics,
) -> GraphNode {
    position_block_lane(block, col, lane as f32, metrics)
}

fn position_block_lane(
    block: &BlockBlueprint,
    col: usize,
    lane: f32,
    metrics: &GridMetrics,
) -> GraphNode {
    GraphNode {
        id: block.id.clone(),
        label: block.label.clone(),
        category: block.category,
        x: metrics.origin_x + col as f32 * metrics.column_spacing,
        y: metrics.origin_y + lane * metrics.lane_spacing,
        bypass: block.bypass,
    }
}
