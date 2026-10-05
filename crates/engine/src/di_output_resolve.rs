//! Responsibility: resolves which output a chain's DI loop or looper plays to.
//! A saved `(binding id, endpoint name)` reference may name ANY output endpoint
//! of the project, not only the chain's own: the isolated stream opens on that
//! endpoint's device and channels. `chain_index` is where the reference sits
//! among the chain's resolved outputs (the order
//! [`crate::runtime_endpoints::resolve_chain_io`] numbers output streams with),
//! used where playback still mixes into a chain output stream (JACK).
//!
//! `None`, a stale binding id, or a stale endpoint name all fall back to the
//! chain's main (first) output, so legacy projects keep today's routing.

use domain::io_binding::IoBinding;
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::Chain;

use crate::runtime_endpoints::OutputEntry;

/// Where an isolated playback goes.
#[derive(Debug, Clone, PartialEq)]
pub struct IsolatedOutput {
    /// Flat index among the chain's own outputs; `0` when the target is not
    /// one of them.
    pub chain_index: usize,
    /// The device and channels the playback lands on.
    pub entry: OutputEntry,
}

/// Resolve a saved output reference for `chain`. `None` only when the
/// reference names nothing and the chain has no output either.
pub fn resolve_isolated_output(
    chain: &Chain,
    registry: &[IoBinding],
    saved: Option<(&str, &str)>,
) -> Option<IsolatedOutput> {
    let chain_outputs: Vec<_> = resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|p| p.direction == PortDirection::Output)
        .collect();
    let target = saved.and_then(|(binding_id, endpoint)| {
        let ep = registry
            .iter()
            .find(|b| b.id == binding_id)?
            .outputs
            .iter()
            .find(|ep| ep.name == endpoint)?;
        Some((binding_id, ep))
    });
    match target {
        Some((binding_id, ep)) => {
            let chain_index = chain_outputs
                .iter()
                .position(|p| p.binding_id == binding_id && p.endpoint.name == ep.name)
                .or_else(|| {
                    chain_outputs.iter().position(|p| {
                        p.endpoint.device_id == ep.device_id && p.endpoint.channels == ep.channels
                    })
                })
                .unwrap_or(0);
            Some(IsolatedOutput {
                chain_index,
                entry: OutputEntry::from(ep),
            })
        }
        None => chain_outputs.first().map(|p| IsolatedOutput {
            chain_index: 0,
            entry: OutputEntry::from(&p.endpoint),
        }),
    }
}

#[cfg(test)]
#[path = "di_output_resolve_tests.rs"]
mod tests;
