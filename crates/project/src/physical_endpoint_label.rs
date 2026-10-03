//! Responsibility: names a physical endpoint the way every picker shows it.

use domain::AudioDeviceDescriptor;

/// `"{device} · {direction} {channels}"`, channels 1-based and joined by `/`.
/// A device the host no longer lists shows its id.
pub fn physical_endpoint_label(
    direction: &str,
    device_id: &str,
    channels: &[usize],
    devices: &[AudioDeviceDescriptor],
) -> String {
    let device = devices
        .iter()
        .find(|d| d.id == device_id)
        .map(|d| d.name.as_str())
        .unwrap_or(device_id);
    let channels = channels
        .iter()
        .map(|c| (c + 1).to_string())
        .collect::<Vec<_>>()
        .join("/");
    format!("{device} · {direction} {channels}")
}

#[cfg(test)]
#[path = "physical_endpoint_label_tests.rs"]
mod tests;
