//! #716 discovery: resolve a chain's audio I/O PORTS from the I/O bindings it
//! selects (`chain.io_binding_ids`) plus its mid Input/Output blocks — never
//! from per-block device endpoints.
//!
//! The chain's MAIN start/end I/O is never persisted: the head inputs and tail
//! outputs are materialized here from the selected bindings. Mid `Input` /
//! `Output` blocks (manually inserted) reference a binding by `io`/`endpoint`.
//! Each resolved port carries the device endpoint (device_id/channels/mode)
//! taken from the per-machine binding registry — the single source of truth
//! for the chain's I/O.

use domain::io_binding::{IoBinding, IoEndpoint};

use crate::block::AudioBlockKind;
use crate::chain::Chain;

/// Direction of a resolved chain I/O port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
}

/// A resolved I/O port of a chain: where it sits in the chain, which binding
/// (E/S) it belongs to, and the device endpoint (resolved from the registry)
/// it reads from / writes to.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainPort {
    pub direction: PortDirection,
    /// Position in the chain's block list. `0` = head (before all blocks),
    /// `chain.blocks.len()` = tail (after all blocks); otherwise the index of
    /// the mid `Input`/`Output` block that produced this port.
    pub offset: usize,
    /// Whether a mid `Input`/`Output` BLOCK produced this port. A head port
    /// sits before block 0 and a mid port at offset 0 sits after it — the
    /// offset alone cannot tell them apart (#85).
    pub from_block: bool,
    /// Id of the binding (E/S) this port belongs to.
    pub binding_id: String,
    /// Device endpoint resolved from the binding.
    pub endpoint: IoEndpoint,
}

/// Resolve every I/O port of `chain` against the binding `registry`.
///
/// - Head inputs / tail outputs come from the bindings the chain selects
///   (`io_binding_ids`) — these are never persisted in the chain.
/// - Mid `Input` / `Output` blocks resolve their `io`/`endpoint` reference.
///
/// Ports whose binding (or endpoint) is absent from the registry are skipped.
pub fn resolve_chain_ports(chain: &Chain, registry: &[IoBinding]) -> Vec<ChainPort> {
    let find = |id: &str| registry.iter().find(|b| b.id == id);
    let tail = chain.blocks.len();
    let mut ports = Vec::new();

    // Head inputs + tail outputs come from the bindings the chain selects.
    for binding_id in &chain.io_binding_ids {
        let Some(binding) = find(binding_id) else {
            continue; // selection references a binding not in the registry → skip
        };
        for ep in &binding.inputs {
            ports.push(ChainPort {
                direction: PortDirection::Input,
                offset: 0,
                from_block: false,
                binding_id: binding.id.clone(),
                endpoint: ep.clone(),
            });
        }
        for ep in &binding.outputs {
            ports.push(ChainPort {
                direction: PortDirection::Output,
                offset: tail,
                from_block: false,
                binding_id: binding.id.clone(),
                endpoint: ep.clone(),
            });
        }
    }

    // Mid Input/Output blocks reference one binding endpoint by io/endpoint.
    for (i, block) in chain.blocks.iter().enumerate() {
        let (direction, io, endpoint_name) = match &block.kind {
            AudioBlockKind::Input(b) => (PortDirection::Input, &b.io, &b.endpoint),
            AudioBlockKind::Output(b) => (PortDirection::Output, &b.io, &b.endpoint),
            _ => continue,
        };
        let Some(binding) = find(io) else {
            continue;
        };
        let pool = match direction {
            PortDirection::Input => &binding.inputs,
            PortDirection::Output => &binding.outputs,
        };
        let Some(ep) = pool.iter().find(|e| &e.name == endpoint_name) else {
            continue;
        };
        ports.push(ChainPort {
            direction,
            offset: i,
            from_block: true,
            binding_id: binding.id.clone(),
            endpoint: ep.clone(),
        });
    }

    ports
}

/// #323: flat index of a looper's chosen INPUT endpoint among the chain's
/// resolved inputs — the same deterministic order the engine numbers input
/// segments with (mirror of `engine::di_output_resolve` for outputs). `None`,
/// a stale binding id, or a stale endpoint name all fall back to `0` (the
/// chain's first input), so a fresh looper and legacy projects record the
/// first input, unchanged.
pub fn resolve_input_segment(
    chain: &Chain,
    registry: &[IoBinding],
    input: Option<&crate::chain::EndpointRef>,
) -> usize {
    let Some(target) = input else {
        return 0;
    };
    resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|p| p.direction == PortDirection::Input)
        .position(|p| p.binding_id == target.binding_id && p.endpoint.name == target.endpoint)
        .unwrap_or(0)
}

/// #323: flat index of a looper's chosen OUTPUT endpoint among the chain's
/// resolved outputs (the same order the engine numbers output routes with).
/// `None` / stale ⇒ `0` (the chain's main output).
pub fn resolve_output_segment(
    chain: &Chain,
    registry: &[IoBinding],
    output: Option<&crate::chain::EndpointRef>,
) -> usize {
    let Some(target) = output else {
        return 0;
    };
    resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|p| p.direction == PortDirection::Output)
        .position(|p| p.binding_id == target.binding_id && p.endpoint.name == target.endpoint)
        .unwrap_or(0)
}

/// #323: the chain's bound input / output endpoint labels, in the deterministic
/// order the selectors index into. Each label is the endpoint name, prefixed
/// with the binding's NAME (not its id) only when the same endpoint name
/// repeats across bindings — mirrors the DI output picker.
pub fn chain_endpoint_labels(chain: &Chain, registry: &[IoBinding]) -> (Vec<String>, Vec<String>) {
    let ports = resolve_chain_ports(chain, registry);
    let binding_name = |id: &str| -> String {
        registry
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.name.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| id.to_string())
    };
    let label = |port: &crate::binding_discovery::ChainPort| -> String {
        let same_name = ports
            .iter()
            .filter(|q| q.direction == port.direction && q.endpoint.name == port.endpoint.name)
            .count()
            > 1;
        if same_name {
            format!("{} · {}", binding_name(&port.binding_id), port.endpoint.name)
        } else {
            port.endpoint.name.clone()
        }
    };
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for port in &ports {
        match port.direction {
            PortDirection::Input => inputs.push(label(port)),
            PortDirection::Output => outputs.push(label(port)),
        }
    }
    (inputs, outputs)
}

/// #323: the `EndpointRef` for the input at flat index `index`, or `None` when
/// out of range (which a caller treats as "the chain's first input").
pub fn input_endpoint_ref(
    chain: &Chain,
    registry: &[IoBinding],
    index: usize,
) -> Option<crate::chain::EndpointRef> {
    endpoint_ref(chain, registry, PortDirection::Input, index)
}

/// #323: the `EndpointRef` for the output at flat index `index`.
pub fn output_endpoint_ref(
    chain: &Chain,
    registry: &[IoBinding],
    index: usize,
) -> Option<crate::chain::EndpointRef> {
    endpoint_ref(chain, registry, PortDirection::Output, index)
}

fn endpoint_ref(
    chain: &Chain,
    registry: &[IoBinding],
    direction: PortDirection,
    index: usize,
) -> Option<crate::chain::EndpointRef> {
    resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|p| p.direction == direction)
        .nth(index)
        .map(|p| crate::chain::EndpointRef {
            binding_id: p.binding_id,
            endpoint: p.endpoint.name,
        })
}

#[cfg(test)]
#[path = "binding_discovery_looper_tests.rs"]
mod looper_tests;
