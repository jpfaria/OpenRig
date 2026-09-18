//! Responsibility: reports what each output route's device stream pulled.
//!
//! #923: the per-stream meters say what a SEGMENT produced; nothing said
//! whether the device stream owning a route ever popped it, or how loud what
//! it popped was. When one tail output plays and its sibling on the same
//! runtime stays silent, this is the number that tells the engine apart from
//! the backend: a route with callbacks and level that is inaudible was lost
//! after our code.
//!
//! RT side: one `Relaxed` add and one `fetch_max` per output callback, both on
//! the route the callback already holds — no allocation, no lock (invariant
//! #8). Read side: plain atomic loads off the audio thread.

use std::sync::atomic::Ordering;

use crate::output_meter::SILENT_DBFS;
use crate::runtime_state::{ChainRuntimeState, OutputRoutingState};

/// One output route as its device stream saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputRouteStats {
    /// Position among the runtime's routes — the `output_index` the stream
    /// pops with.
    pub route: usize,
    /// Device channels the route writes.
    pub channels: Vec<usize>,
    /// Output callbacks served since the route was built.
    pub callbacks: u64,
    /// Empty pops since the route was built (see `ElasticBuffer`).
    pub underruns: u64,
    /// Loudest |sample| popped since the previous read, in dBFS;
    /// [`SILENT_DBFS`] when nothing was popped.
    pub peak_dbfs: f32,
    /// #953: frames queued in the route's cushion right now — its latency
    /// beyond the device buffer. Sibling routes of one runtime that disagree
    /// here play the same signal apart.
    pub fill_frames: usize,
    /// #953: times the route shed latency a stalled stream left behind.
    pub latency_trims: u64,
}

impl OutputRoutingState {
    /// Audio thread: account one served callback whose loudest popped
    /// |sample| was `peak`.
    #[inline]
    pub(crate) fn record_callback(&self, peak: f32) {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        self.peak_bits
            .fetch_max(peak.max(0.0).to_bits(), Ordering::Relaxed);
    }
}

impl ChainRuntimeState {
    /// Every route's counters, resetting each route's peak so the next read
    /// reports what was popped in between. Off the audio thread only.
    pub fn take_output_route_stats(&self) -> Vec<OutputRouteStats> {
        self.output_routes
            .load()
            .iter()
            .enumerate()
            .filter_map(|(route, state)| state.as_ref().map(|state| (route, state)))
            .map(|(route, state)| {
                let peak = f32::from_bits(state.peak_bits.swap(0, Ordering::Relaxed));
                OutputRouteStats {
                    route,
                    channels: state.output_channels.clone(),
                    callbacks: state.callbacks.load(Ordering::Relaxed),
                    underruns: state.buffer.underrun_count(),
                    peak_dbfs: if peak > 0.0 {
                        20.0 * peak.log10()
                    } else {
                        SILENT_DBFS
                    },
                    fill_frames: state.buffer.len(),
                    latency_trims: state.buffer.latency_trims(),
                }
            })
            .collect()
    }
}
