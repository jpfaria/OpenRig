//! Responsibility: routes the `GraphView` model to the file that owns each job
//!
//! Was one 552-line file declaring "data model and layout helpers" — the
//! conjunction was the tell (#895). One job per file now:
//!
//! - [`types`] — what a graph IS
//! - [`palette`] — what colour a category gets
//! - [`chain_builder`] — building a positioned graph from chain stages
//! - [`routing_ids`] — the names of the split and merge routing nodes
//! - [`layout`] — placing existing nodes by edge topology
//! - [`validation`] — what makes a graph description ill-formed
//! - [`reorder`] — moving a dragged node among its column siblings
//! - [`anchors`] — the insert anchors on a laid-out chain's wires
//! - [`drop_target`] — which anchor a dragged block lands on

mod anchors;
mod chain_builder;
mod drop_target;
mod layout;
mod palette;
mod reorder;
mod routing_ids;
mod types;
mod validation;

pub use anchors::{insert_anchors, AnchorSlot, GraphAnchor};
pub use chain_builder::linear_chain_layout;
pub use drop_target::resolve_drop_anchor;
pub use layout::topological_layout;
pub use palette::{default_palette, CategoryStyle};
pub use reorder::reorder_for_drop;
pub use types::{
    BlockBlueprint, ChainStage, GraphEdge, GraphNode, GridMetrics, NodeCategory, NodeKind,
    ParallelEnd,
};
pub use validation::{validate_graph, validate_stages};

#[cfg(test)]
#[path = "../graph_view_model_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../graph_view_model_anchor_tests.rs"]
mod anchor_tests;

#[cfg(test)]
#[path = "../graph_view_model_drop_tests.rs"]
mod drop_tests;
