//! #791: objective audio-quality report query for one chain.
//!
//! Read-side parity for the Tone Doctor's Layer 3 measurement: runs the
//! synthetic test-signal battery through the deterministic offline render
//! (`engine::chain_quality::measure_quality`) and serialises THD+N, noise
//! floor, level, dynamic range and clipping as a JSON envelope — the same
//! numbers every transport (GUI / MCP / gRPC) sees. Kept out of `query.rs`
//! (already at its line cap) as a focused module.

use domain::ids::ChainId;
use project::project::Project;

/// Fixed measurement conditions so the report is reproducible regardless of the
/// live audio device.
const REPORT_SAMPLE_RATE: f32 = 48_000.0;
const REPORT_BLOCK_SIZE: usize = 512;

/// Objective quality report for `chain`, as `{"quality": {…}}`. Unknown chain
/// → `Err`.
pub fn chain_quality_report(project: &Project, chain: &ChainId) -> Result<String, String> {
    let chain_ref = project
        .chains
        .iter()
        .find(|c| c.id == *chain)
        .ok_or_else(|| format!("chain not found: {}", chain.0))?;

    let m = engine::chain_quality::measure_quality(chain_ref, REPORT_SAMPLE_RATE, REPORT_BLOCK_SIZE)
        .map_err(|e| format!("quality measurement failed: {e}"))?;

    let envelope = serde_json::json!({
        "quality": {
            "thd_n": m.thd_n,
            "noise_floor_dbfs": m.noise_floor_dbfs,
            "peak_dbfs": m.peak_dbfs,
            "rms_dbfs": m.rms_dbfs,
            "dynamic_range_db": m.dynamic_range_db,
            "clip_fraction": m.clip_fraction,
        }
    });
    Ok(envelope.to_string())
}

#[cfg(test)]
#[path = "query_chain_quality_tests.rs"]
mod tests;
