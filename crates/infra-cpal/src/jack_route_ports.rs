//! Responsibility: names the JACK output ports each output route of a chain owns.
//! Linux/JACK-direct runs one client for a chain's whole runtime (#328). Each
//! output route of that runtime gets its OWN set of output ports, one per
//! device channel, each connected to the device's playback port for that
//! channel. When two routes write the same channel, JACK sums their ports at
//! the playback port — the backend mix CLAUDE.md allows; our code never adds
//! two routes together. Route 0 keeps the historical `out_N` names, so a
//! single-output chain registers exactly the ports it always did.

/// One JACK output port of a route and the playback port it feeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutePort {
    /// Short port name registered on the chain's client.
    pub(crate) name: String,
    /// Full name of the device playback port it is connected to.
    pub(crate) playback: String,
}

/// The ports one output route of the runtime writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutePorts {
    /// The route index handed to `process_output_f32`.
    pub(crate) route: usize,
    /// One port per device channel, in channel order.
    pub(crate) ports: Vec<RoutePort>,
}

/// The port layout of a runtime with `route_count` output routes on a
/// device with `channels` playback channels. A runtime without routes still
/// gets route 0's ports, the shape every JACK chain had before #328.
pub(crate) fn route_ports(route_count: usize, channels: usize) -> Vec<RoutePorts> {
    (0..route_count.max(1))
        .map(|route| RoutePorts {
            route,
            ports: (0..channels)
                .map(|ch| RoutePort {
                    name: port_name(route, ch),
                    playback: format!("system:playback_{}", ch + 1),
                })
                .collect(),
        })
        .collect()
}

fn port_name(route: usize, channel: usize) -> String {
    if route == 0 {
        format!("out_{}", channel + 1)
    } else {
        format!("route{}_out_{}", route, channel + 1)
    }
}

#[cfg(test)]
#[path = "jack_route_ports_tests.rs"]
mod jack_route_ports_tests;
