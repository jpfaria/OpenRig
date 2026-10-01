//! Responsibility: answers which anchor a dragged graph card lands on.
//!
//! #328 (spec §5.1). GraphView asks on every drag move (`resolve-drop-anchor`,
//! Part 5) so the target lights up before release, and fires `node-dropped`
//! with the answer. The geometry is Part 5's `resolve_drop_anchor`; this lays
//! the row's chain out again — the same stages and grid its row was published
//! from — and asks it.

use project::chain::Chain;

use crate::chain_graph_adapter::{chain_graph, grid_metrics};
use crate::endpoint_checklist_items::IoLabels;
use crate::graph_view_model::resolve_drop_anchor;

/// The anchor id a card `node_id` dragged to layout `(x, y)` lands on, or `""`.
pub(crate) fn drop_anchor_id(chain: &Chain, node_id: &str, x: f32, y: f32) -> String {
    // Labels move no node and no anchor, so none are resolved here.
    let labels = IoLabels::default();
    let graph = chain_graph(chain, &labels);
    resolve_drop_anchor(
        &graph.nodes,
        &graph.anchors,
        node_id,
        x,
        y,
        grid_metrics(graph.lanes),
    )
    .map(|anchor| anchor.id.clone())
    .unwrap_or_default()
}

#[cfg(test)]
#[path = "chain_graph_drop_tests.rs"]
mod tests;
