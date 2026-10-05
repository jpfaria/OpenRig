//! Responsibility: lists the physical outputs a project's bindings expose.

use domain::io_binding::IoBinding;
use domain::AudioDeviceDescriptor;

/// One selectable output: a device and a set of its channels, reached through
/// an output endpoint of one of the project's I/O bindings. Every output
/// picker (backing-track player, metronome, DI, looper) lists these, so they
/// all offer the same outputs under the same names.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectOutput {
    /// The binding whose endpoint names this output first.
    pub binding_id: String,
    /// That endpoint's name.
    pub endpoint: String,
    /// Stable key `"{binding_id}\u{1f}{endpoint}"`, round-tripped by the
    /// pickers that persist a string.
    pub key: String,
    /// `"{device name} · Out {channels}"` (channels 1-based, e.g. `Out 1/2`),
    /// shown in the picker.
    pub label: String,
    pub device_id: String,
    pub channels: Vec<usize>,
    /// Keys of the other endpoints that route to these same device channels.
    /// They are not listed again, but a reference to one of them still
    /// resolves here.
    pub aliases: Vec<String>,
}

impl ProjectOutput {
    /// Whether `key` names this output, directly or through an alias.
    pub fn answers_to(&self, key: &str) -> bool {
        self.key == key || self.aliases.iter().any(|a| a == key)
    }
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
    bindings: &[IoBinding],
    devices: &[AudioDeviceDescriptor],
) -> Vec<ProjectOutput> {
    let mut outputs: Vec<ProjectOutput> = Vec::new();
    for binding in bindings {
        for endpoint in &binding.outputs {
            let key = endpoint_key(&binding.id, &endpoint.name);
            let device_id = endpoint.device_id.0.as_str();
            match outputs
                .iter_mut()
                .find(|o| o.device_id == device_id && o.channels == endpoint.channels)
            {
                Some(existing) => existing.aliases.push(key),
                None => outputs.push(ProjectOutput {
                    binding_id: binding.id.clone(),
                    endpoint: endpoint.name.clone(),
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

fn output_label(device_id: &str, channels: &[usize], devices: &[AudioDeviceDescriptor]) -> String {
    let device = devices
        .iter()
        .find(|d| d.id == device_id)
        .map_or(device_id, |d| d.name.as_str());
    let channels: Vec<String> = channels.iter().map(|c| (c + 1).to_string()).collect();
    format!("{device} · Out {}", channels.join("/"))
}

/// Position of the output a `(binding id, endpoint name)` reference names,
/// directly or through an alias. `None` when it names no output any more.
pub fn output_position(
    outputs: &[ProjectOutput],
    binding_id: &str,
    endpoint: &str,
) -> Option<usize> {
    let key = endpoint_key(binding_id, endpoint);
    outputs.iter().position(|o| o.answers_to(&key))
}

/// Resolve the saved endpoint key to a concrete output: the saved one while it
/// still exists, otherwise the first endpoint (a renamed binding or a different
/// machine must not leave the player silent). `None` only when the project
/// has no output endpoint at all.
pub fn resolve_output_endpoint(
    saved: Option<&str>,
    endpoints: &[ProjectOutput],
) -> Option<ProjectOutput> {
    saved
        .and_then(|key| endpoints.iter().find(|o| o.answers_to(key)).cloned())
        .or_else(|| endpoints.first().cloned())
}

#[cfg(test)]
#[path = "project_outputs_tests.rs"]
mod tests;
