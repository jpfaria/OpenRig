//! Responsibility: moves a dragged card within its row's published graph.
//!
//! #328 (spec §5.1). Part 5's GraphView follows the pointer only through its
//! host's model (`node_dragged`), so the dragged node — and the ends of its
//! wires — move in place, row by row, in the models `chain_graph_models`
//! published. A rebuild would recreate the card mid-drag.

use slint::{Model, VecModel};

use crate::{GraphEdgeGeometry, GraphNode, ProjectChainItem};

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

#[cfg(test)]
#[path = "chain_graph_drag_tests.rs"]
mod tests;
