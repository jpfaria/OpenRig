//! Responsibility: folds pipelines that run the same processing on one jack into one fan-out pipeline.
//!
//! #1074: a segment is built per (input × output) pair, so one guitar wired to
//! three outputs — through one E/S or through several E/S reading the same
//! jack — ran the whole chain three times. Two segments that read the same
//! jack, run the same blocks and the same split paths produce the same signal;
//! keeping both only multiplies the DSP, the taps and the latency spread
//! between the outputs. They fold into ONE pipeline writing every route.
//!
//! The fold never crosses an output DEVICE: another interface runs on its own
//! clock, and its pipeline stays its own (invariant #4). Two different jacks
//! never fold either — they are two guitars.

use crate::runtime_endpoints::OutputEntry;
use crate::segment_types::ChainSegment;

/// Fold `segments` that do the same work on the same jack for the same output
/// device into one, in first-seen order. A route already fed by the folded
/// pipeline is not added twice.
pub(crate) fn fold_same_jack_segments(
    segments: Vec<ChainSegment>,
    outputs: &[OutputEntry],
) -> Vec<ChainSegment> {
    let mut folded: Vec<ChainSegment> = Vec::with_capacity(segments.len());
    for segment in segments {
        match folded
            .iter_mut()
            .find(|kept| runs_the_same_work(kept, &segment, outputs))
        {
            Some(kept) => {
                for route in segment.output_route_indices {
                    if !kept.output_route_indices.contains(&route) {
                        kept.output_route_indices.push(route);
                    }
                }
            }
            None => folded.push(segment),
        }
    }
    folded
}

fn runs_the_same_work(a: &ChainSegment, b: &ChainSegment, outputs: &[OutputEntry]) -> bool {
    a.mid_output_taps.is_empty()
        && b.mid_output_taps.is_empty()
        && a.input == b.input
        && a.cpal_input_index == b.cpal_input_index
        && a.block_indices == b.block_indices
        && a.split_mono_sibling_count == b.split_mono_sibling_count
        && a.paths == b.paths
        && output_device(a, outputs).is_some()
        && output_device(a, outputs) == output_device(b, outputs)
}

/// The one device every route of `segment` writes to; `None` when its routes
/// span several devices or point past the outputs.
fn output_device<'a>(segment: &ChainSegment, outputs: &'a [OutputEntry]) -> Option<&'a str> {
    let mut device = None;
    for &route in &segment.output_route_indices {
        let id = outputs.get(route)?.device_id.0.as_str();
        match device {
            None => device = Some(id),
            Some(seen) if seen == id => {}
            Some(_) => return None,
        }
    }
    device
}
