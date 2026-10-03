//! Responsibility: lists the endpoints the metronome can play to.

use domain::distinct_endpoints::distinct_endpoints;

/// One selectable metronome output: an output endpoint of one of the project's
/// I/O bindings (#14). The metronome plays through the SAME outputs the project
/// is configured with, not a raw device list — so it lands on the channels the
/// user already set up.
#[derive(Debug, Clone, PartialEq)]
pub struct MetronomeOutput {
    /// Stable key `"{binding_id}\u{1f}{endpoint_name}"`, round-tripped by the
    /// select and persisted in `config.yaml`.
    pub key: String,
    /// The endpoint name, prefixed with its binding's name only when two
    /// different physical outputs share it.
    pub label: String,
    pub device_id: String,
    pub channels: Vec<usize>,
    /// Keys of the other bindings that carry this same physical output, so a
    /// key saved through any copy still resolves to it.
    pub aliases: Vec<String>,
}

/// The key that identifies an output endpoint. The unit separator keeps it
/// unambiguous even if a binding id or endpoint name contains a space or dot.
pub fn endpoint_key(binding_id: &str, endpoint_name: &str) -> String {
    format!("{binding_id}\u{1f}{endpoint_name}")
}

/// Every physical output the project's bindings expose, once each, in
/// registry order (`domain::distinct_endpoints`).
pub fn output_endpoints(bindings: &[infra_filesystem::IoBinding]) -> Vec<MetronomeOutput> {
    let listed = bindings
        .iter()
        .flat_map(|binding| binding.outputs.iter().map(move |e| (binding, e)));
    distinct_endpoints(listed)
        .into_iter()
        .map(|d| MetronomeOutput {
            key: endpoint_key(&d.binding_id, &d.endpoint),
            label: d.label,
            device_id: d.device_id.0,
            channels: d.channels,
            aliases: d
                .aliases
                .iter()
                .skip(1)
                .map(|a| endpoint_key(&a.binding_id, &a.endpoint))
                .collect(),
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
        .and_then(|key| {
            endpoints
                .iter()
                .find(|o| o.key == key || o.aliases.iter().any(|a| a == key))
                .cloned()
        })
        .or_else(|| endpoints.first().cloned())
}
