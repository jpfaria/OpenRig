//! Responsibility: lists the distinct physical endpoints a chain can pick from.
//! A chain's I/O bindings overlap: the same guitar input or the same MAIN
//! output is carried by several of them. A pick (looper, DI output) names a
//! physical endpoint, so each one is offered once, in the chain's resolved
//! port order (`domain::distinct_endpoints`).

use domain::distinct_endpoints::{distinct_endpoints, position_of, DistinctEndpoint};
use domain::io_binding::IoBinding;

use crate::binding_discovery::{resolve_chain_ports, PortDirection};
use crate::chain::{Chain, EndpointRef};

/// The chain's distinct endpoints in `direction`, in resolved port order.
pub fn chain_endpoint_options(
    chain: &Chain,
    registry: &[IoBinding],
    direction: PortDirection,
) -> Vec<DistinctEndpoint> {
    let ports: Vec<_> = resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|p| p.direction == direction)
        .filter_map(|p| {
            let binding = registry.iter().find(|b| b.id == p.binding_id)?;
            Some((binding, p.endpoint))
        })
        .collect();
    distinct_endpoints(ports.iter().map(|(b, e)| (*b, e)))
}

/// The chain's input / output option labels.
pub fn chain_endpoint_labels(chain: &Chain, registry: &[IoBinding]) -> (Vec<String>, Vec<String>) {
    let labels = |direction| {
        chain_endpoint_options(chain, registry, direction)
            .into_iter()
            .map(|d| d.label)
            .collect()
    };
    (labels(PortDirection::Input), labels(PortDirection::Output))
}

/// The `EndpointRef` behind input option `index`; `None` when out of range.
pub fn input_option_ref(
    chain: &Chain,
    registry: &[IoBinding],
    index: usize,
) -> Option<EndpointRef> {
    option_ref(chain, registry, PortDirection::Input, index)
}

/// The `EndpointRef` behind output option `index`; `None` when out of range.
pub fn output_option_ref(
    chain: &Chain,
    registry: &[IoBinding],
    index: usize,
) -> Option<EndpointRef> {
    option_ref(chain, registry, PortDirection::Output, index)
}

/// The input option an endpoint shows as. `None` or stale ⇒ `0`.
pub fn input_option_index(
    chain: &Chain,
    registry: &[IoBinding],
    input: Option<&EndpointRef>,
) -> usize {
    option_index(chain, registry, PortDirection::Input, input)
}

/// The output option an endpoint shows as. `None` or stale ⇒ `0`.
pub fn output_option_index(
    chain: &Chain,
    registry: &[IoBinding],
    output: Option<&EndpointRef>,
) -> usize {
    option_index(chain, registry, PortDirection::Output, output)
}

fn option_ref(
    chain: &Chain,
    registry: &[IoBinding],
    direction: PortDirection,
    index: usize,
) -> Option<EndpointRef> {
    chain_endpoint_options(chain, registry, direction)
        .into_iter()
        .nth(index)
        .map(|d| EndpointRef {
            binding_id: d.binding_id,
            endpoint: d.endpoint,
        })
}

fn option_index(
    chain: &Chain,
    registry: &[IoBinding],
    direction: PortDirection,
    target: Option<&EndpointRef>,
) -> usize {
    target
        .and_then(|t| {
            position_of(
                &chain_endpoint_options(chain, registry, direction),
                &t.binding_id,
                &t.endpoint,
            )
        })
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "chain_endpoint_options_tests.rs"]
mod tests;
