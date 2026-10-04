//! Responsibility: lists what a graph input or output node shows.
//!
//! #328 (spec §5.3). The input node stands for every input of the chain's E/S
//! bindings; an output node for every output — the same set
//! `endpoint_candidates` lists, unchecked ones included (`resolve_chain_ports`
//! drops an unchecked endpoint, so it cannot feed a checklist). A mid-chain
//! `Input`/`Output` port block is a card of its own, not part of these nodes.
//! A row is checked unless THIS node disabled it (`Chain.disabled_endpoints`,
//! Part 1) — nothing is added to or removed from the E/S.

use domain::distinct_endpoints::distinct_endpoints;
use domain::AudioDeviceDescriptor;
use infra_filesystem::IoBinding;
use project::block::{y_leaves, PathRef};
use project::chain::Chain;
use project::endpoint_disables::{EndpointNode, EndpointRef};
use project::physical_endpoint_label::physical_endpoint_label;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EndpointRow {
    pub(crate) io: String,
    pub(crate) endpoint: String,
    pub(crate) label: String,
    pub(crate) enabled: bool,
    /// Every binding copy of this physical endpoint; a toggle switches them all.
    pub(crate) aliases: Vec<EndpointRef>,
}

/// One row per physical endpoint (`domain::distinct_endpoints`): bindings
/// that overlap repeat the same device + channels, and the row stands for
/// every copy. It is checked while any copy still plays. A row is named by
/// device and channels ("Quantum HD 8 · Out 1/2"), like the output pickers.
pub(crate) fn endpoint_rows(
    chain: &Chain,
    registry: &[IoBinding],
    devices: &[AudioDeviceDescriptor],
    node: &EndpointNode,
) -> Vec<EndpointRow> {
    let direction = match node {
        EndpointNode::Input => "In",
        EndpointNode::Output | EndpointNode::PathOutput(_) => "Out",
    };
    let listed = chain
        .io_binding_ids
        .iter()
        .filter_map(|id| registry.iter().find(|b| &b.id == id))
        .flat_map(|binding| {
            let endpoints = match node {
                EndpointNode::Input => &binding.inputs,
                EndpointNode::Output | EndpointNode::PathOutput(_) => &binding.outputs,
            };
            endpoints.iter().map(move |endpoint| (binding, endpoint))
        });
    distinct_endpoints(listed)
        .into_iter()
        .map(|distinct| {
            let aliases: Vec<EndpointRef> = distinct
                .aliases
                .iter()
                .map(|a| EndpointRef {
                    io: a.binding_id.clone(),
                    endpoint: a.endpoint.clone(),
                })
                .collect();
            EndpointRow {
                enabled: aliases
                    .iter()
                    .any(|alias| chain.disabled_endpoints.is_enabled(node, alias)),
                io: distinct.binding_id,
                endpoint: distinct.endpoint,
                label: physical_endpoint_label(
                    direction,
                    &distinct.device_id.0,
                    &distinct.channels,
                    devices,
                ),
                aliases,
            }
        })
        .collect()
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

/// The text of the endpoint nodes a chain graph has: its input, its output
/// and the output node of each Y leaf (#328 §11.3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct IoLabels {
    pub(crate) input: String,
    pub(crate) output: String,
    pub(crate) leaves: Vec<(PathRef, String)>,
}

impl IoLabels {
    /// The text of leaf `leaf`'s output node; empty when it has none.
    pub(crate) fn leaf(&self, leaf: &PathRef) -> &str {
        self.leaves
            .iter()
            .find(|(path, _)| path == leaf)
            .map(|(_, text)| text.as_str())
            .unwrap_or_default()
    }
}

/// Rows republished without a registry (many callers pass `&[]` to
/// `replace_project_chains`) cannot name endpoints; they keep the row's own
/// `input_label` / `output_label` text instead of claiming "none".
pub(crate) fn io_labels(
    chain: &Chain,
    registry: &[IoBinding],
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
    fallback_input: &str,
    fallback_output: &str,
) -> IoLabels {
    let none = rust_i18n::t!("label-endpoints-none").to_string();
    let label = |node: &EndpointNode, fallback: &str| {
        let devices = match node {
            EndpointNode::Input => input_devices,
            EndpointNode::Output | EndpointNode::PathOutput(_) => output_devices,
        };
        let rows = endpoint_rows(chain, registry, devices, node);
        if rows.is_empty() {
            fallback.to_string()
        } else {
            node_label(&rows, &none)
        }
    };
    IoLabels {
        input: label(&EndpointNode::Input, fallback_input),
        output: label(&EndpointNode::Output, fallback_output),
        leaves: y_leaves(&chain.blocks)
            .into_iter()
            .map(|leaf| {
                let text = label(&EndpointNode::PathOutput(leaf.clone()), fallback_output);
                (leaf, text)
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "endpoint_checklist_items_tests.rs"]
mod tests;
