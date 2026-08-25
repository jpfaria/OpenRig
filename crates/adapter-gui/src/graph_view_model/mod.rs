//! Responsibility: routes the `GraphView` model to the file that owns each job
//!
//! Was one 552-line file declaring "data model and layout helpers" — the
//! conjunction was the tell (#895). One job per file now:
//!
//! - [`types`] — what a graph IS
//! - [`palette`] — what colour a category gets
//! - [`chain_builder`] — building a positioned graph from chain stages
//! - [`layout`] — placing existing nodes by edge topology
//! - [`validation`] — what makes a graph ill-formed
//! - [`reorder`] — moving a dragged node among its column siblings

mod chain_builder;
mod layout;
mod palette;
mod reorder;
mod types;
mod validation;

pub use chain_builder::linear_chain_layout;
pub use layout::topological_layout;
pub use palette::{default_palette, CategoryStyle};
pub use reorder::reorder_for_drop;
pub use types::{BlockBlueprint, ChainStage, GraphEdge, GraphNode, GridMetrics, NodeCategory};
pub use validation::validate_graph;

#[cfg(test)]
#[path = "../graph_view_model_tests.rs"]
mod tests;
