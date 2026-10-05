//! Responsibility: moves a dragged card within its row's published graph.
//!
//! #328 (spec §5.1). Part 5's GraphView follows the pointer only through its
//! host's model (`node_dragged`), so the dragged node — and the ends of its
//! wires — move in place, row by row, in the models `chain_graph_models`
//! published. A rebuild would recreate the card mid-drag.

use slint::{Model, VecModel};

use crate::{GraphAnchor, GraphEdgeGeometry, GraphNode, ProjectChainItem};

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
    let ids = crate::chain_graph_split_group::group_node_ids(chain, &split);
    for id in &ids {
        if let Some((nx, ny)) = at(id) {
            move_node(rows, chain_index, id, nx + dx, ny + dy);
        }
    }
    let splits = crate::chain_graph_split_group::group_split_ids(chain, &split);
    shift_lane_marks(&row, &ids, &splits, dx, dy);
}

/// The "+" of every lane the dragged split holds, and the bend an empty
/// lane's wire makes through its own row, move with the split: neither
/// belongs to a card, so `move_node` never reaches them.
fn shift_lane_marks(
    row: &ProjectChainItem,
    group: &[String],
    splits: &[domain::ids::BlockId],
    dx: f32,
    dy: f32,
) {
    let prefixes: Vec<String> = splits.iter().map(|s| format!("path:{}:", s.0)).collect();
    if let Some(anchors) = row
        .graph_anchors
        .as_any()
        .downcast_ref::<VecModel<GraphAnchor>>()
    {
        for i in 0..anchors.row_count() {
            let Some(mut anchor) = anchors.row_data(i) else {
                continue;
            };
            if prefixes.iter().any(|p| anchor.id.starts_with(p.as_str())) {
                anchor.layout_x += dx;
                anchor.layout_y += dy;
                anchors.set_row_data(i, anchor);
            }
        }
    }
    let held = |id: &str| group.iter().any(|g| g == id);
    if let Some(edges) = row
        .graph_edges
        .as_any()
        .downcast_ref::<VecModel<GraphEdgeGeometry>>()
    {
        for i in 0..edges.row_count() {
            let Some(mut edge) = edges.row_data(i) else {
                continue;
            };
            if edge.from_id.is_empty() && held(edge.to_id.as_str()) {
                edge.from_x += dx;
                edge.from_y += dy;
            } else if edge.to_id.is_empty() && held(edge.from_id.as_str()) {
                edge.to_x += dx;
                edge.to_y += dy;
            } else {
                continue;
            }
            edges.set_row_data(i, edge);
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
    let graph = crate::chain_graph_adapter::chain_graph(chain, &labels);
    for node in &graph.nodes {
        move_node(rows, chain_index, &node.id, node.x, node.y);
    }
    let Some(row) = rows.row_data(chain_index) else {
        return;
    };
    let laid = crate::chain_graph_models::row_graph_models(chain, &graph);
    settle_model::<GraphAnchor>(&row.graph_anchors, &laid.anchors);
    settle_model::<GraphEdgeGeometry>(&row.graph_edges, &laid.edges);
}

/// Puts every row of `published` back to the laid-out `laid` row.
fn settle_model<T: Clone + PartialEq + 'static>(
    published: &slint::ModelRc<T>,
    laid: &slint::ModelRc<T>,
) {
    let Some(rows) = published.as_any().downcast_ref::<VecModel<T>>() else {
        return;
    };
    if rows.row_count() != laid.row_count() {
        return;
    }
    for (i, row) in laid.iter().enumerate() {
        if rows.row_data(i).as_ref() != Some(&row) {
            rows.set_row_data(i, row);
        }
    }
}

#[cfg(test)]
#[path = "chain_graph_drag_tests.rs"]
mod tests;
