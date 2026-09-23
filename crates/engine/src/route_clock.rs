//! Responsibility: tells which device clocks feed each output route.
//!
//! A route's frames come from the segments that write it, each fed by one
//! input device, and leave through the route's output device. When every
//! writer's input is that same device, producer and consumer share one clock
//! and the route's fill never drifts. Otherwise the two clocks drift apart
//! even at the same nominal rate, and the ring slowly drains or fills.

use domain::ids::DeviceId;
use domain::io_binding::IoBinding;
use project::chain::Chain;

use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_segments::{split_chain_into_segments, ChainSegment};

/// The input devices of the segments writing route `route_idx` (at its tail
/// or through a mid tap), each listed once.
fn producer_devices(segments: &[ChainSegment], route_idx: usize) -> Vec<DeviceId> {
    let mut devices: Vec<DeviceId> = Vec::new();
    for segment in segments.iter().filter(|segment| {
        segment.output_route_indices.contains(&route_idx)
            || segment
                .mid_output_taps
                .iter()
                .any(|tap| tap.route_idx == route_idx)
    }) {
        if !devices.contains(&segment.input.device_id) {
            devices.push(segment.input.device_id.clone());
        }
    }
    devices
}

/// Whether every segment writing route `route_idx` reads its input from
/// `output_device`.
pub(crate) fn route_on_producer_clock(
    segments: &[ChainSegment],
    route_idx: usize,
    output_device: &DeviceId,
) -> bool {
    matches!(producer_devices(segments, route_idx).as_slice(), [only] if only == output_device)
}

/// For each of the chain's output streams — regular outputs first, then one
/// per bound Insert send, the order the streams are opened in — the input
/// devices whose streams feed it. The stream layer sizes a route's cushion
/// from ITS producers only (#965: sizing it from every chain input gave a
/// route fed from another clock the one-buffer cushion of a same-clock route,
/// and let one stream's buffer size raise another stream's latency).
pub fn route_producers(chain: &Chain, registry: &[IoBinding]) -> Vec<Vec<DeviceId>> {
    let (resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let (effective_ins, cpal_indices, split_positions, entry_groups) =
        effective_inputs(chain, &resolved_inputs, registry);
    let effective_outs = effective_outputs(chain, &resolved_outputs, registry);
    let segments = split_chain_into_segments(
        chain,
        &effective_ins,
        &cpal_indices,
        &split_positions,
        &entry_groups,
        &effective_outs,
        registry,
    );
    (0..effective_outs.len())
        .map(|route_idx| producer_devices(&segments, route_idx))
        .collect()
}

#[cfg(test)]
#[path = "route_clock_tests.rs"]
mod tests;
