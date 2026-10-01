//! Responsibility: moves a dragged card within its row's published graph.
//!
//! #328 (spec §5.1). Part 5's GraphView follows the pointer only through its
//! host's model (`node_dragged`), so the dragged node — and the ends of its
//! wires — move in place, row by row, in the models `chain_graph_models`
//! published. A rebuild would recreate the card mid-drag.

use slint::{Model, VecModel};

use crate::{GraphEdgeGeometry, GraphNode, ProjectChainItem};

/// A card dragged to layout `(x, y)`. A split or mixer card carries its
/// whole split by the same offset; any other card moves alone.
pub(crate) fn drag_node(
    rows: &VecModel<ProjectChainItem>,
    chain_index: usize,
    chain: &project::chain::Chain,
    node_id: &str,
    x: f32,
    y: f32,
) {
    let Some(split) = crate::chain_graph_split_group::dragged_split(chain, node_id) else {
        move_node(rows, chain_index, node_id, x, y);
        return;
    };
    let Some(row) = rows.row_data(chain_index) else {
        return;
    };
    let at = |id: &str| {
        row.graph_nodes
            .iter()
            .find(|n| n.id.as_str() == id)
            .map(|n| (n.layout_x, n.layout_y))
    };
    let Some((from_x, from_y)) = at(node_id) else {
        return;
    };
    let (dx, dy) = (x - from_x, y - from_y);
    for id in crate::chain_graph_split_group::group_node_ids(chain, &split) {
        if let Some((nx, ny)) = at(&id) {
            move_node(rows, chain_index, &id, nx + dx, ny + dy);
        }
    }
}

pub(crate) fn move_node(
    rows: &VecModel<ProjectChainItem>,
    chain_index: usize,
    node_id: &str,
    x: f32,
    y: f32,
) {
    let Some(row) = rows.row_data(chain_index) else {
        return;
    };
    if let Some(nodes) = row
        .graph_nodes
        .as_any()
        .downcast_ref::<VecModel<GraphNode>>()
    {
        for i in 0..nodes.row_count() {
            let Some(mut node) = nodes.row_data(i) else {
                continue;
            };
            if node.id.as_str() == node_id {
                node.layout_x = x;
                node.layout_y = y;
                nodes.set_row_data(i, node);
            }
        }
    }
    if let Some(edges) = row
        .graph_edges
        .as_any()
        .downcast_ref::<VecModel<GraphEdgeGeometry>>()
    {
        for i in 0..edges.row_count() {
            let Some(mut edge) = edges.row_data(i) else {
                continue;
            };
            let mut moved = false;
            if edge.from_id.as_str() == node_id {
                (edge.from_x, edge.from_y, moved) = (x, y, true);
            }
            if edge.to_id.as_str() == node_id {
                (edge.to_x, edge.to_y, moved) = (x, y, true);
            }
            if moved {
                edges.set_row_data(i, edge);
            }
        }
    }
}

/// When a drag is released, every card returns to `chain`'s laid-out grid.
/// A drop that moved a block has already republished the row, so this only
/// undoes a drag that did not.
pub(crate) fn settle_nodes(
    rows: &VecModel<ProjectChainItem>,
    chain_index: usize,
    chain: &project::chain::Chain,
) {
    // Labels move no node, so none are resolved here.
    let labels = crate::endpoint_checklist_items::IoLabels::default();
    for node in crate::chain_graph_adapter::chain_graph(chain, &labels).nodes {
        move_node(rows, chain_index, &node.id, node.x, node.y);
    }
}

#[cfg(test)]
#[path = "chain_graph_drag_tests.rs"]
mod tests;
