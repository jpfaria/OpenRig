//! Responsibility: resolves which insert anchor a dragged block lands on
//!
//! The canvas asks through a pure callback (the #787 `slot-at` pattern) on
//! every drag move, to light the target up, and emits `node-dropped` with
//! the answer on release — so the geometry lives here, once. Mapping the
//! anchor's slot onto a command belongs to the host.

use super::anchors::GraphAnchor;
use super::types::{GraphNode, GridMetrics, NodeKind};

/// The anchor a block `dragged_id` released at layout `(x, y)` lands on:
/// the nearest anchor closer than half a column.
///
/// `None` when the nearest anchor sits on the block's own wire (landing
/// there leaves the chain unchanged — it must not fall through to the next
/// nearest, which can be the other lane's), when the dragged node is not a
/// [`NodeKind::Block`] (I/O, split and mixer nodes do not move), when it is
/// unknown, or when no anchor is in reach.
pub fn resolve_drop_anchor<'a>(
    nodes: &[GraphNode],
    anchors: &'a [GraphAnchor],
    dragged_id: &str,
    x: f32,
    y: f32,
    metrics: GridMetrics,
) -> Option<&'a GraphAnchor> {
    let dragged = nodes.iter().find(|n| n.id == dragged_id)?;
    if dragged.kind != NodeKind::Block {
        return None;
    }
    let reach = metrics.column_spacing / 2.0;
    let (nearest, _) = anchors
        .iter()
        .map(|a| (a, (a.x - x).powi(2) + (a.y - y).powi(2)))
        .filter(|(_, distance_sq)| *distance_sq < reach * reach)
        .min_by(|(_, l), (_, r)| l.total_cmp(r))?;
    if nearest.from_id == dragged_id || nearest.to_id == dragged_id {
        return None;
    }
    Some(nearest)
}
