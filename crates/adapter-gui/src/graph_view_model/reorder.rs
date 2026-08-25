//! Responsibility: reorders a dragged node among its column siblings
//!
//! Re-tidy after a drag in auto mode. The dragged node keeps its
//! edge-derived column; the drop's Y only decides where it lands among the
//! nodes sharing that column.

use super::layout::topological_layout;
use super::types::{GraphEdge, GraphNode, GridMetrics};

/// Returns a fresh [`topological_layout`] with the dragged node reordered.
/// Panic-free: an unknown id yields the clean layout unchanged.
pub fn reorder_for_drop(
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    dragged_id: &str,
    drop_y: f32,
    metrics: GridMetrics,
) -> Vec<GraphNode> {
    let laid = topological_layout(nodes, edges, metrics);
    let Some(dragged) = laid.iter().find(|n| n.id == dragged_id) else {
        return laid;
    };
    let dragged_x = dragged.x;
    let mut siblings: Vec<&GraphNode> = laid.iter().filter(|n| n.x == dragged_x).collect();
    if siblings.len() < 2 {
        return laid;
    }
    siblings.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    let order: Vec<&str> = siblings.iter().map(|n| n.id.as_str()).collect();
    let cur = order.iter().position(|id| *id == dragged_id).unwrap();
    let mut target = 0usize;
    let mut best = f32::MAX;
    for (i, s) in siblings.iter().enumerate() {
        let d = (s.y - drop_y).abs();
        if d < best {
            best = d;
            target = i;
        }
    }
    if target == cur {
        return laid;
    }
    let mut new_sib_ids: Vec<&str> = order
        .iter()
        .filter(|id| **id != dragged_id)
        .copied()
        .collect();
    new_sib_ids.insert(target.min(new_sib_ids.len()), dragged_id);
    let sib_set: std::collections::HashSet<&str> = order.iter().copied().collect();
    let mut sib_iter = new_sib_ids.iter();
    let feed: Vec<GraphNode> = nodes
        .iter()
        .map(|src| {
            if sib_set.contains(src.id.as_str()) {
                let next = *sib_iter.next().unwrap();
                nodes.iter().find(|n| n.id == next).unwrap().clone()
            } else {
                src.clone()
            }
        })
        .collect();
    topological_layout(&feed, edges, metrics)
}
