//! Responsibility: builds a positioned graph from a chain's stages
//!
//! Layout strategy:
//!
//! - [`super::types::ChainStage::Single`] blocks sit on the central lane and
//!   advance the column cursor by one.
//! - `Parallel` places each inner path on its own lane (above/below the
//!   centre, distributed symmetrically) and reserves columns equal to the
//!   longest path. Split/merge utility nodes are inserted automatically so
//!   the result is a connected DAG.

use super::types::{BlockBlueprint, ChainStage, GraphEdge, GraphNode, GridMetrics, NodeCategory};

/// Build a positioned graph from a sequence of [`ChainStage`]s.
///
/// Returns the (nodes, edges) pair ready to push to the Slint side. IDs
/// must be unique across the whole input — duplicates produce undefined
/// behaviour at the UI level (the panic-free contract is kept here, but
/// the UI may render only one of the duplicates).
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
            ChainStage::Parallel(paths) if paths.is_empty() => {
                // No-op — nothing to render, no column consumed.
            }
            ChainStage::Parallel(paths) => {
                split_counter += 1;
                let split_id = format!("__split_{split_counter}");
                let merge_id = format!("__merge_{split_counter}");

                let longest = paths.iter().map(Vec::len).max().unwrap_or(0);
                let split_col = col;
                let merge_col = col + longest + 1;

                // Split node sits at split_col on the centre lane.
                nodes.push(GraphNode {
                    id: split_id.clone(),
                    label: String::new(),
                    category: NodeCategory::Util,
                    x: metrics.origin_x + split_col as f32 * metrics.column_spacing,
                    y: metrics.origin_y,
                    bypass: false,
                });
                if let Some(prev) = prev_tail.take() {
                    edges.push(GraphEdge {
                        from_id: prev,
                        to_id: split_id.clone(),
                    });
                }

                // Each path occupies its own lane. With N paths, lanes
                // are -N/2..N/2 around the centre; 2 paths → -0.5 / +0.5.
                let n_paths = paths.len() as f32;
                for (lane_idx, path) in paths.iter().enumerate() {
                    let lane_offset = lane_idx as f32 - (n_paths - 1.0) / 2.0;
                    let mut last_in_lane = split_id.clone();
                    for (block_idx, block) in path.iter().enumerate() {
                        let node = position_block_lane(
                            block,
                            split_col + 1 + block_idx,
                            lane_offset,
                            &metrics,
                        );
                        edges.push(GraphEdge {
                            from_id: last_in_lane,
                            to_id: node.id.clone(),
                        });
                        last_in_lane = node.id.clone();
                        nodes.push(node);
                    }
                    edges.push(GraphEdge {
                        from_id: last_in_lane,
                        to_id: merge_id.clone(),
                    });
                }

                // Merge node sits at merge_col on the centre lane.
                nodes.push(GraphNode {
                    id: merge_id.clone(),
                    label: String::new(),
                    category: NodeCategory::Util,
                    x: metrics.origin_x + merge_col as f32 * metrics.column_spacing,
                    y: metrics.origin_y,
                    bypass: false,
                });
                prev_tail = Some(merge_id);
                col = merge_col + 1;
            }
        }
    }

    (nodes, edges)
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
