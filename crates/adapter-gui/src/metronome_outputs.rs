//! Responsibility: lists the endpoints the metronome can play to.

/// One selectable metronome output: an output endpoint of one of the project's
/// I/O bindings (#14). The metronome plays through the SAME outputs the project
/// is configured with, not a raw device list — so it lands on the channels the
/// user already set up.
#[derive(Debug, Clone, PartialEq)]
pub struct MetronomeOutput {
    /// Stable key `"{binding_id}\u{1f}{endpoint_name}"`, round-tripped by the
    /// select and persisted in `config.yaml`.
    pub key: String,
    /// `"{binding name} · {endpoint name}"`, shown in the picker.
    pub label: String,
    pub device_id: String,
    pub channels: Vec<usize>,
}

/// The key that identifies an output endpoint. The unit separator keeps it
/// unambiguous even if a binding id or endpoint name contains a space or dot.
pub fn endpoint_key(binding_id: &str, endpoint_name: &str) -> String {
    format!("{binding_id}\u{1f}{endpoint_name}")
}

/// Every output endpoint the project's bindings expose, in registry order.
pub fn output_endpoints(bindings: &[infra_filesystem::IoBinding]) -> Vec<MetronomeOutput> {
    bindings
        .iter()
        .flat_map(|binding| {
            binding.outputs.iter().map(move |endpoint| MetronomeOutput {
                key: endpoint_key(&binding.id, &endpoint.name),
                label: format!("{} · {}", binding.name, endpoint.name),
                device_id: endpoint.device_id.0.clone(),
                channels: endpoint.channels.clone(),
            })
        })
        .collect()
}

/// Resolve the saved endpoint key to a concrete output: the saved one while it
/// still exists, otherwise the first endpoint (a renamed binding or a different
/// machine must not leave the metronome silent). `None` only when the project
/// has no output endpoint at all.
pub fn resolve_output_endpoint(
    saved: Option<&str>,
    endpoints: &[MetronomeOutput],
) -> Option<MetronomeOutput> {
    saved
        .and_then(|key| endpoints.iter().find(|o| o.key == key).cloned())
        .or_else(|| endpoints.first().cloned())
}
