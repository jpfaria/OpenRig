//! Responsibility: decides which chain node uses each endpoint of the chain's bindings.
//!
//! #328 — the input and output nodes of a chain's graph each carry a checklist
//! of the endpoints of the chain's own E/S (`Chain.disabled_endpoints`, copied
//! from `RigInput`). Every endpoint is checked unless the node's list names
//! it. A chain with no Y split anywhere has one output node; a chain holding a
//! Y at any depth has one output node per Y leaf (spec §11.3) and no chain
//! output node. A reference to an endpoint the E/S no longer has matches
//! nothing, so it is ignored.

use domain::io_binding::IoBinding;

use crate::block::{y_leaves, AudioBlockKind, PathRef};
use crate::chain::Chain;
use crate::endpoint_disables::{EndpointNode, EndpointRef};

/// Which output node of a chain sends to one tail endpoint of its E/S.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailFeed {
    /// No node sends here: the endpoint is unchecked on every output node.
    Off,
    /// The chain output node (chains without a Y split).
    Chain,
    /// The Y leaves whose output node has the endpoint checked — never empty.
    Leaves(Vec<PathRef>),
}

/// Whether the chain's input node reads head endpoint `endpoint` of E/S `io`.
pub fn head_input_enabled(chain: &Chain, io: &str, endpoint: &str) -> bool {
    chain
        .disabled_endpoints
        .is_enabled(&EndpointNode::Input, &endpoint_ref(io, endpoint))
}

/// Which output node of `chain` sends to tail endpoint `endpoint` of E/S `io`.
pub fn tail_feed(chain: &Chain, io: &str, endpoint: &str) -> TailFeed {
    let reference = endpoint_ref(io, endpoint);
    let disables = &chain.disabled_endpoints;
    let leaves = y_leaves(&chain.blocks);
    if leaves.is_empty() {
        return if disables.is_enabled(&EndpointNode::Output, &reference) {
            TailFeed::Chain
        } else {
            TailFeed::Off
        };
    }
    let fed: Vec<PathRef> = leaves
        .into_iter()
        .filter(|leaf| disables.leaf_output_enabled(leaf, &reference))
        .collect();
    if fed.is_empty() {
        TailFeed::Off
    } else {
        TailFeed::Leaves(fed)
    }
}

/// Whether the input node's checklist is what leaves the chain with no
/// source: its E/S have head inputs, every one is unchecked, and no enabled
/// mid `Input` block brings a signal of its own.
pub fn inputs_all_unchecked(chain: &Chain, registry: &[IoBinding]) -> bool {
    let mut heads = selected_bindings(chain, registry)
        .flat_map(|binding| {
            binding
                .inputs
                .iter()
                .map(move |ep| (binding.id.as_str(), ep.name.as_str()))
        })
        .peekable();
    heads.peek().is_some()
        && heads.all(|(io, endpoint)| !head_input_enabled(chain, io, endpoint))
        && !has_enabled_block(chain, |kind| matches!(kind, AudioBlockKind::Input(_)))
}

/// Whether the output nodes' checklists are what leave the chain with no
/// output: its E/S have tail outputs, no output node sends to any of them,
/// and no enabled mid `Output` block takes the signal elsewhere.
pub fn outputs_all_unchecked(chain: &Chain, registry: &[IoBinding]) -> bool {
    let mut tails = selected_bindings(chain, registry)
        .flat_map(|binding| {
            binding
                .outputs
                .iter()
                .map(move |ep| (binding.id.as_str(), ep.name.as_str()))
        })
        .peekable();
    tails.peek().is_some()
        && tails.all(|(io, endpoint)| tail_feed(chain, io, endpoint) == TailFeed::Off)
        && !has_enabled_block(chain, |kind| matches!(kind, AudioBlockKind::Output(_)))
}

/// Whether the checklist leaves the chain nothing to play (spec §5.3:
/// allowed — the chain simply builds no segment).
pub fn checklist_silences(chain: &Chain, registry: &[IoBinding]) -> bool {
    inputs_all_unchecked(chain, registry) || outputs_all_unchecked(chain, registry)
}

fn has_enabled_block(chain: &Chain, is_port: fn(&AudioBlockKind) -> bool) -> bool {
    chain
        .blocks
        .iter()
        .any(|block| block.enabled && is_port(&block.kind))
}

fn selected_bindings<'a>(
    chain: &'a Chain,
    registry: &'a [IoBinding],
) -> impl Iterator<Item = &'a IoBinding> {
    chain
        .io_binding_ids
        .iter()
        .filter_map(move |id| registry.iter().find(|binding| &binding.id == id))
}

fn endpoint_ref(io: &str, endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: io.to_string(),
        endpoint: endpoint.to_string(),
    }
}

#[cfg(test)]
#[path = "endpoint_feeds_tests.rs"]
mod tests;
