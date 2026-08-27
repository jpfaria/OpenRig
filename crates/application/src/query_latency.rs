//! Responsibility: reports the measured latency of a chain.
//! #829: the per-chain latency probe as a query.
//!
//! Clicking the sonar badge ran `engine::probe` on the GUI thread and wrote
//! the number onto a chain row — the measurement existed nowhere else. The
//! resolution of *which* rate and buffer to probe at (issue #723: never
//! assume 48 kHz) lives here so the GUI and every other transport measure
//! the same way instead of each re-deriving it.

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use project::project::Project;
use serde::Serialize;

/// Probe buffer used when the chain's input device has no saved setting.
/// Matches the GUI badge's historical default.
pub const DEFAULT_PROBE_BUFFER_FRAMES: usize = 256;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LatencyReport {
    pub chain: String,
    /// Rate the probe ran at — the chain input device's saved rate, else
    /// the live engine rate.
    pub sample_rate: f32,
    pub buffer_frames: usize,
    /// DSP latency through the chain, in milliseconds.
    pub dsp_latency_ms: f32,
}

/// Resolve the probe settings for `chain` and measure it.
///
/// `live_sample_rate` is the rate the streams actually run at — the
/// authoritative fallback for an input with no saved per-device setting.
pub fn chain_latency_report(
    project: &Project,
    bindings: &[IoBinding],
    chain_id: &ChainId,
    live_sample_rate: f32,
) -> Result<String, String> {
    let report = measure_chain_latency(project, bindings, chain_id, live_sample_rate)?;
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

/// The measurement itself. The GUI badge reads the struct; transports read
/// [`chain_latency_report`]'s JSON — one resolution of rate and buffer, not
/// two.
pub fn measure_chain_latency(
    project: &Project,
    bindings: &[IoBinding],
    chain_id: &ChainId,
    live_sample_rate: f32,
) -> Result<LatencyReport, String> {
    let chain = project
        .chains
        .iter()
        .find(|c| &c.id == chain_id)
        .ok_or_else(|| format!("unknown chain: {}", chain_id.0))?;

    let (resolved_inputs, _) = engine::runtime_endpoints::resolve_chain_io(chain, bindings);
    let device = resolved_inputs.first().and_then(|entry| {
        project
            .device_settings
            .iter()
            .find(|d| d.device_id == entry.device_id)
    });
    let sample_rate = device
        .map(|d| d.sample_rate as f32)
        .unwrap_or(live_sample_rate);
    let buffer_frames = device
        .map(|d| d.buffer_size_frames as usize)
        .unwrap_or(DEFAULT_PROBE_BUFFER_FRAMES);

    Ok(LatencyReport {
        chain: chain.id.0.clone(),
        sample_rate,
        buffer_frames,
        dsp_latency_ms: engine::probe::measure_chain_dsp_latency_ms(
            chain,
            sample_rate,
            buffer_frames,
        ),
    })
}

#[cfg(test)]
#[path = "query_latency_tests.rs"]
mod tests;
