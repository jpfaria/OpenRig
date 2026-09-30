//! Responsibility: lists what a graph input or output node shows.
//!
//! #328 (spec §5.3). The input node stands for every input of the chain's E/S
//! bindings; an output node for every output — Part 1's `endpoint_candidates`,
//! which lists them unchecked ones included (`resolve_chain_ports` drops an
//! unchecked endpoint, so it cannot feed a checklist). A mid-chain
//! `Input`/`Output` port block is a card of its own, not part of these nodes.
//! A row is checked unless THIS node disabled it (`Chain.disabled_endpoints`,
//! Part 1) — nothing is added to or removed from the E/S.

use infra_filesystem::IoBinding;
use project::chain::Chain;
use project::endpoint_candidates::endpoint_candidates;
use project::endpoint_disables::{EndpointNode, EndpointRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EndpointRow {
    pub(crate) io: String,
    pub(crate) endpoint: String,
    pub(crate) label: String,
    pub(crate) enabled: bool,
}

pub(crate) fn endpoint_rows(
    chain: &Chain,
    registry: &[IoBinding],
    node: EndpointNode,
) -> Vec<EndpointRow> {
    let (inputs, outputs) = endpoint_candidates(&chain.io_binding_ids, registry);
    let refs = match node {
        EndpointNode::Input => inputs,
        EndpointNode::Output | EndpointNode::PathAOutput | EndpointNode::PathBOutput => outputs,
    };
    refs.iter()
        .map(|reference| EndpointRow {
            label: row_label(reference, &refs, registry),
            enabled: chain.disabled_endpoints.is_enabled(node, reference),
            io: reference.io.clone(),
            endpoint: reference.endpoint.clone(),
        })
        .collect()
}

/// The endpoint name, prefixed with its binding's NAME when the same endpoint
/// name repeats among the listed endpoints — the rule `chain_endpoint_labels`
/// uses for the looper and DI selects.
fn row_label(reference: &EndpointRef, refs: &[EndpointRef], registry: &[IoBinding]) -> String {
    let repeated = refs
        .iter()
        .filter(|other| other.endpoint == reference.endpoint)
        .count()
        > 1;
    if !repeated {
        return reference.endpoint.clone();
    }
    let binding = registry
        .iter()
        .find(|b| b.id == reference.io)
        .map(|b| b.name.trim())
        .filter(|name| !name.is_empty())
        .unwrap_or(reference.io.as_str());
    format!("{binding} · {}", reference.endpoint)
}

/// The node's text: its checked endpoints, or `none` when every one is off.
pub(crate) fn node_label(rows: &[EndpointRow], none: &str) -> String {
    let names: Vec<&str> = rows
        .iter()
        .filter(|row| row.enabled)
        .map(|row| row.label.as_str())
        .collect();
    if names.is_empty() {
        none.to_string()
    } else {
        names.join(", ")
    }
}

/// The text of the four endpoint nodes a chain graph can have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IoLabels {
    pub(crate) input: String,
    pub(crate) output: String,
    pub(crate) path_a: String,
    pub(crate) path_b: String,
}

/// Rows republished without a registry (many callers pass `&[]` to
/// `replace_project_chains`) cannot name endpoints; they keep the row's own
/// `input_label` / `output_label` text instead of claiming "none".
pub(crate) fn io_labels(
    chain: &Chain,
    registry: &[IoBinding],
    fallback_input: &str,
    fallback_output: &str,
) -> IoLabels {
    let none = rust_i18n::t!("label-endpoints-none").to_string();
    let label = |node: EndpointNode, fallback: &str| {
        let rows = endpoint_rows(chain, registry, node);
        if rows.is_empty() {
            fallback.to_string()
        } else {
            node_label(&rows, &none)
        }
    };
    IoLabels {
        input: label(EndpointNode::Input, fallback_input),
        output: label(EndpointNode::Output, fallback_output),
        path_a: label(EndpointNode::PathAOutput, fallback_output),
        path_b: label(EndpointNode::PathBOutput, fallback_output),
    }
}

#[cfg(test)]
#[path = "endpoint_checklist_items_tests.rs"]
mod tests;
