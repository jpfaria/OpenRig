//! Responsibility: lays out the JACK ports of the drum machine's own client.

use crate::jack_route_ports::RoutePort;

/// One output port per channel of the drums' stereo pair (one for a mono
/// endpoint), each feeding that channel's playback port.
pub(crate) fn drums_jack_ports(targets: &[usize]) -> Vec<RoutePort> {
    let channels: &[usize] = if targets.is_empty() { &[0, 1] } else { targets };
    channels
        .iter()
        .take(2)
        .enumerate()
        .map(|(i, ch)| RoutePort {
            name: format!("out_{}", i + 1),
            playback: format!("system:playback_{}", ch + 1),
        })
        .collect()
}

/// The JACK server an output endpoint lives on: `jack:<server>` names it,
/// `hw:<card>` is looked up through `hw_server`.
pub(crate) fn jack_server_for_device(
    device_id: &str,
    hw_server: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    if let Some(name) = device_id.strip_prefix("jack:") {
        return Some(name.to_string());
    }
    device_id.strip_prefix("hw:").and_then(hw_server)
}

#[cfg(test)]
#[path = "drums_jack_ports_tests.rs"]
mod tests;
