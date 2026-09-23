//! Responsibility: tells whether an output route runs on its producer's device clock.
//!
//! A route's frames come from the segments that write it, each fed by one
//! input device, and leave through the route's output device. When every
//! writer's input is that same device, producer and consumer share one clock
//! and the route's fill never drifts. Otherwise the two clocks drift apart
//! even at the same nominal rate, and the ring slowly drains or fills.

use domain::ids::DeviceId;

use crate::segment_types::ChainSegment;

/// Whether every segment writing route `route_idx` (at its tail or through a
/// mid tap) reads its input from `output_device`.
pub(crate) fn route_on_producer_clock(
    segments: &[ChainSegment],
    route_idx: usize,
    output_device: &DeviceId,
) -> bool {
    let mut writers = segments
        .iter()
        .filter(|segment| {
            segment.output_route_indices.contains(&route_idx)
                || segment
                    .mid_output_taps
                    .iter()
                    .any(|tap| tap.route_idx == route_idx)
        })
        .peekable();
    writers.peek().is_some() && writers.all(|segment| &segment.input.device_id == output_device)
}
