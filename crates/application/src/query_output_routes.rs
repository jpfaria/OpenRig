//! Responsibility: reports what each output route's device stream pulled.
//!
//! #923: the per-chain meters say what each segment produced; this read says
//! whether the device stream owning each output ROUTE ever ran, how many
//! times, how often it found the cushion empty, and the loudest sample it
//! carried since the previous read. Served on the frontend that hosts the
//! runtime (`openrig://routes` over MCP), the documented empty shape
//! elsewhere.

use engine::runtime_output_route_stats::OutputRouteStats;
use serde::Serialize;

/// One output route of one per-input runtime, as its device stream saw it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutputRouteReading {
    pub chain: String,
    /// The per-input runtime (cpal input group) that owns the route.
    pub group: usize,
    /// Position among that runtime's routes — the stream's `output_index`.
    pub route: usize,
    /// Device channels the route writes.
    pub channels: Vec<usize>,
    /// Output callbacks served since the route was built.
    pub callbacks: u64,
    /// Empty pops since the route was built.
    pub underruns: u64,
    /// Loudest |sample| popped since the previous read, in dBFS.
    pub peak_dbfs: f32,
    /// #953: frames queued in the route's cushion right now.
    pub fill_frames: usize,
    /// #953: times the route shed latency a stalled stream left behind.
    pub latency_trims: u64,
    /// #980: frames the route's ring discarded because it was full.
    pub dropped_frames: u64,
    /// #980: input buffers the owning runtime lost on a failed processing
    /// `try_lock` (per runtime, repeated on each of its rows).
    pub input_busy_skips: u64,
}

#[derive(Serialize)]
struct RoutesPayload<'a> {
    hosted: bool,
    rows: &'a [OutputRouteReading],
}

/// One chain's runtime groups, flattened into the rows `openrig://routes`
/// lists: group order, then route order — the same order the streams pop in.
pub fn rows_for_chain(
    chain: &str,
    groups: Vec<(usize, Vec<OutputRouteStats>)>,
) -> Vec<OutputRouteReading> {
    groups
        .into_iter()
        .flat_map(|(group, routes)| {
            routes.into_iter().map(move |r| OutputRouteReading {
                chain: chain.to_string(),
                group,
                route: r.route,
                channels: r.channels,
                callbacks: r.callbacks,
                underruns: r.underruns,
                peak_dbfs: r.peak_dbfs,
                fill_frames: r.fill_frames,
                latency_trims: r.latency_trims,
                dropped_frames: r.dropped_frames,
                input_busy_skips: r.input_busy_skips,
            })
        })
        .collect()
}

pub fn output_routes_json(hosted: bool, rows: &[OutputRouteReading]) -> String {
    serde_json::to_string(&RoutesPayload { hosted, rows })
        .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

#[cfg(test)]
#[path = "query_output_routes_tests.rs"]
mod tests;
