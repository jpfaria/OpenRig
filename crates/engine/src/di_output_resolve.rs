//! #771: resolve a chain's persisted DI output choice (`Chain.di_output`,
//! a `DiOutputRef { binding_id, endpoint }`) to the FLAT index of that output
//! among the chain's resolved outputs — the same deterministic order
//! [`crate::runtime_endpoints::resolve_chain_io`] numbers output streams with.
//!
//! `None`, a stale binding id, or a stale endpoint name all fall back to `0`
//! (the chain's main/first output), so legacy projects keep today's routing.

use domain::io_binding::IoBinding;
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::{Chain, DiOutputRef};

/// Flat output index the chain's DI player must mix into.
pub fn resolve_di_output_index(
    chain: &Chain,
    registry: &[IoBinding],
    di_output: Option<&DiOutputRef>,
) -> usize {
    let Some(target) = di_output else {
        return 0;
    };
    let flat_index = resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|p| p.direction == PortDirection::Output)
        .position(|p| p.binding_id == target.binding_id && p.endpoint.name == target.endpoint);
    flat_index.unwrap_or(0)
}

#[cfg(test)]
#[path = "di_output_resolve_tests.rs"]
mod tests;
