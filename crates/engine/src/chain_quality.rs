//! Objective chain-quality measurement (#791, Layer 3; was #609).
//!
//! Pushes the deterministic test-signal battery through the offline render and
//! hands each output to [`feature_dsp::quality_metrics`] for measurement. Unlike
//! the blame-by-ablation path, this uses synthetic signals (not the player's
//! DI) and answers "is this chain objectively clean?" — THD+N, noise floor,
//! level, dynamic range, clipping.
//!
//! Offline and deterministic: reuses [`crate::offline::render_chain`], so no
//! audio-thread work and per-chain isolation by construction.

use anyhow::Result;

use feature_dsp::quality_metrics::{assemble, BatterySignal, QualityMetrics};
use project::chain::Chain;

use crate::offline::render_chain;

/// Duration of each battery probe (seconds). One second gives the THD+N FFT a
/// full 48k-sample window at 48 kHz.
const PROBE_SECS: f32 = 1.0;

/// Render one mono battery signal through the chain and return the output
/// collapsed back to mono (mean of L/R).
fn render_probe(
    chain: &Chain,
    signal: BatterySignal,
    sample_rate: f32,
    block_size: usize,
) -> Result<Vec<f32>> {
    let mono = signal.generate(PROBE_SECS, sample_rate);
    let input: Vec<[f32; 2]> = mono.iter().map(|&s| [s, s]).collect();
    let outcome = render_chain(chain, sample_rate, &input, block_size, 0)?;
    Ok(outcome.samples.iter().map(|f| 0.5 * (f[0] + f[1])).collect())
}

/// Measure a chain's objective quality by running the synthetic battery through
/// it offline.
pub fn measure_quality(
    chain: &Chain,
    sample_rate: f32,
    block_size: usize,
) -> Result<QualityMetrics> {
    let sine_out = render_probe(chain, BatterySignal::Sine1k, sample_rate, block_size)?;
    let silence_out = render_probe(chain, BatterySignal::Silence, sample_rate, block_size)?;
    Ok(assemble(&sine_out, &silence_out, sample_rate))
}

#[cfg(test)]
#[path = "chain_quality_tests.rs"]
mod tests;
