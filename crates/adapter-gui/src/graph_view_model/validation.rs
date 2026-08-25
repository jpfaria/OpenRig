//! Responsibility: reports what makes a node-edge pair ill-formed
//!
//! Used to guard layout output before it reaches the UI.

use std::collections::HashMap;

use super::types::{GraphEdge, GraphNode};

/// Validate that the (nodes, edges) pair is a well-formed graph:
///
/// - every node id is unique,
/// - every edge references existing node ids,
/// - no node references itself.
///
/// Returns a list of error messages — empty means valid.
pub fn validate_graph(nodes: &[GraphNode], edges: &[GraphEdge]) -> Vec<String> {
    let mut errors = Vec::new();
    let mut ids: HashMap<&str, usize> = HashMap::new();
    for node in nodes {
        let count = ids.entry(node.id.as_str()).or_insert(0);
        *count += 1;
        if *count == 2 {
            errors.push(format!("duplicate node id: {}", node.id));
        }
    }
    for edge in edges {
        if !ids.contains_key(edge.from_id.as_str()) {
            errors.push(format!("edge references unknown source: {}", edge.from_id));
        }
        if !ids.contains_key(edge.to_id.as_str()) {
            errors.push(format!("edge references unknown target: {}", edge.to_id));
        }
        if edge.from_id == edge.to_id {
            errors.push(format!("self-loop on node: {}", edge.from_id));
        }
    }
    errors
}
