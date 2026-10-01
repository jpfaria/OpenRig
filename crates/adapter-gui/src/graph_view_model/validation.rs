//! Responsibility: reports what makes a graph description ill-formed
//!
//! Guards a chain's stages before layout and the layout output before it
//! reaches the UI.

use std::collections::HashMap;

use super::types::{ChainStage, GraphEdge, GraphNode, ParallelEnd};

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

/// Validate a stage sequence before it is laid out (#328), at any depth:
///
/// - nothing may follow a [`ParallelEnd::Fan`] in the same list — every
///   lane already ended at its own terminal, so a following stage would
///   float unconnected;
/// - a Fan lane may not be empty — its last blueprint IS its terminal.
///
/// Returns a list of error messages — empty means valid.
pub fn validate_stages(stages: &[ChainStage]) -> Vec<String> {
    let mut errors = Vec::new();
    check_list(stages, "", &mut errors);
    errors
}

fn check_list(stages: &[ChainStage], at: &str, errors: &mut Vec<String>) {
    let mut fan_out: Option<usize> = None;
    for (index, stage) in stages.iter().enumerate() {
        if let Some(fan) = fan_out {
            errors.push(format!(
                "stage {at}{index} follows the fan-out at stage {at}{fan} and has no input"
            ));
        }
        let ChainStage::Parallel { lanes, end, .. } = stage else {
            continue;
        };
        for (lane, stages) in lanes.iter().enumerate() {
            if *end == ParallelEnd::Fan && stages.is_empty() {
                errors.push(format!(
                    "fan lane {lane} of stage {at}{index} is empty and has no terminal"
                ));
            }
            check_list(stages, &format!("{at}{index}.{lane}."), errors);
        }
        if *end == ParallelEnd::Fan && !lanes.is_empty() && fan_out.is_none() {
            fan_out = Some(index);
        }
    }
}
