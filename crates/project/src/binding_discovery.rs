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
                binding_id: binding.id.clone(),
                endpoint: ep.clone(),
            });
        }
        for ep in &binding.outputs {
            ports.push(ChainPort {
                direction: PortDirection::Output,
                offset: tail,
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
            binding_id: binding.id.clone(),
            endpoint: ep.clone(),
        });
    }

    ports
}
