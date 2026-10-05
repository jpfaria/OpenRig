//! Responsibility: collapses listed binding endpoints into distinct physical endpoints.
//! I/O bindings overlap: the same guitar input or the same MAIN output is
//! declared by several of them. Every list the user picks from shows a
//! physical endpoint (device + channels) once; the first binding that
//! declares it stands for all, and every copy stays reachable as an alias.

use crate::ids::DeviceId;
use crate::io_binding::{IoBinding, IoEndpoint};

/// One binding's copy of an endpoint: `(binding id, endpoint name)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointAlias {
    pub binding_id: String,
    pub endpoint: String,
}

/// One physical endpoint as a list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistinctEndpoint {
    /// The first binding copy, the one a pick persists.
    pub binding_id: String,
    pub endpoint: String,
    /// The endpoint name, prefixed with its binding's name only when two
    /// different physical endpoints share that name.
    pub label: String,
    pub device_id: DeviceId,
    pub channels: Vec<usize>,
    /// Every listed copy, the first one included.
    pub aliases: Vec<EndpointAlias>,
}

impl DistinctEndpoint {
    /// Whether `(binding_id, endpoint)` is one of this endpoint's copies.
    pub fn carries(&self, binding_id: &str, endpoint: &str) -> bool {
        self.aliases
            .iter()
            .any(|a| a.binding_id == binding_id && a.endpoint == endpoint)
    }
}

/// The distinct physical endpoints among `listed`, in first-seen order.
pub fn distinct_endpoints<'a>(
    listed: impl IntoIterator<Item = (&'a IoBinding, &'a IoEndpoint)>,
) -> Vec<DistinctEndpoint> {
    let mut distinct: Vec<(DistinctEndpoint, String)> = Vec::new();
    for (binding, endpoint) in listed {
        let alias = EndpointAlias {
            binding_id: binding.id.clone(),
            endpoint: endpoint.name.clone(),
        };
        let same = distinct
            .iter_mut()
            .find(|(d, _)| d.device_id == endpoint.device_id && d.channels == endpoint.channels);
        match same {
            Some((d, _)) => {
                if !d.aliases.contains(&alias) {
                    d.aliases.push(alias);
                }
            }
            None => distinct.push((
                DistinctEndpoint {
                    binding_id: binding.id.clone(),
                    endpoint: endpoint.name.clone(),
                    label: String::new(),
                    device_id: endpoint.device_id.clone(),
                    channels: endpoint.channels.clone(),
                    aliases: vec![alias],
                },
                binding_name(binding),
            )),
        }
    }
    let names: Vec<String> = distinct.iter().map(|(d, _)| d.endpoint.clone()).collect();
    distinct
        .into_iter()
        .map(|(mut d, binding)| {
            let clashes = names.iter().filter(|n| **n == d.endpoint).count() > 1;
            d.label = if clashes {
                format!("{binding} · {}", d.endpoint)
            } else {
                d.endpoint.clone()
            };
            d
        })
        .collect()
}

/// The position of the endpoint carrying `(binding_id, endpoint)`.
pub fn position_of(list: &[DistinctEndpoint], binding_id: &str, endpoint: &str) -> Option<usize> {
    list.iter().position(|d| d.carries(binding_id, endpoint))
}

fn binding_name(binding: &IoBinding) -> String {
    let name = binding.name.trim();
    if name.is_empty() {
        binding.id.clone()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
#[path = "distinct_endpoints_tests.rs"]
mod tests;
