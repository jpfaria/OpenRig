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
    /// `"{device name} · Out {channels}"` (channels 1-based, e.g. `Out 1/2`),
    /// shown in the picker.
    pub label: String,
    pub device_id: String,
    pub channels: Vec<usize>,
    /// Keys of the other endpoints that route to these same device channels.
    /// They are not listed again, but a key saved for one of them still
    /// resolves here.
    pub aliases: Vec<String>,
}

/// The key that identifies an output endpoint. The unit separator keeps it
/// unambiguous even if a binding id or endpoint name contains a space or dot.
pub fn endpoint_key(binding_id: &str, endpoint_name: &str) -> String {
    format!("{binding_id}\u{1f}{endpoint_name}")
}

/// Every physical output the project's bindings expose, in registry order.
/// Endpoints of different bindings that route to the same device channels are
/// one output: the first one owns the key, the others become its aliases.
/// `devices` names each device in the label; a device the host no longer lists
/// is shown by its id.
pub fn output_endpoints(
    bindings: &[infra_filesystem::IoBinding],
    devices: &[domain::AudioDeviceDescriptor],
) -> Vec<MetronomeOutput> {
    let mut outputs: Vec<MetronomeOutput> = Vec::new();
    for binding in bindings {
        for endpoint in &binding.outputs {
            let key = endpoint_key(&binding.id, &endpoint.name);
            let device_id = endpoint.device_id.0.as_str();
            match outputs
                .iter_mut()
                .find(|o| o.device_id == device_id && o.channels == endpoint.channels)
            {
                Some(existing) => existing.aliases.push(key),
                None => outputs.push(MetronomeOutput {
                    key,
                    label: output_label(device_id, &endpoint.channels, devices),
                    device_id: device_id.to_string(),
                    channels: endpoint.channels.clone(),
                    aliases: Vec::new(),
                }),
            }
        }
    }
    outputs
}

fn output_label(
    device_id: &str,
    channels: &[usize],
    devices: &[domain::AudioDeviceDescriptor],
) -> String {
    let device = devices
        .iter()
        .find(|d| d.id == device_id)
        .map_or(device_id, |d| d.name.as_str());
    let channels: Vec<String> = channels.iter().map(|c| (c + 1).to_string()).collect();
    format!("{device} · Out {}", channels.join("/"))
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
