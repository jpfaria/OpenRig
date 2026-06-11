//! Handler for [`crate::command::Command::RenderChain`] (issue #576).
//!
//! Render is a Command so every transport adapter (MCP today, gRPC and any
//! other tomorrow) inherits the tool through the schema-derived catalog —
//! same parity contract as state-mutating Commands. The orchestration
//! itself (chain YAML load → WAV decode → engine offline → atomic WAV
//! write) lives in `adapter-render`; this module is the thin shim that
//! converts the Command payload into [`adapter_render::cli::RenderArgs`],
//! invokes the renderer, and translates the result back into
//! [`crate::event::Event::RenderCompleted`].
//!
//! File mode only. Live capture (cpal-backed) stays inside the
//! `openrig-render` binary — keeping `application` free of audio-device
//! deps. An MCP/gRPC client that wants a live capture re-uses the binary
//! out-of-band, then re-runs the Command with the captured WAV.

use std::path::PathBuf;

use adapter_render::cli::RenderArgs;
use adapter_render::{render, RenderError, RenderSummary};
use anyhow::{anyhow, Result};

use crate::event::Event;

const DEFAULT_SAMPLE_RATE_HZ: u32 = 48_000;
const DEFAULT_BLOCK_SIZE: usize = 256;
const DEFAULT_BIT_DEPTH: u8 = 24;
const DEFAULT_TAIL_MS: u32 = 2_000;

/// #693: cheap synchronous validation run by the dispatch arm BEFORE
/// spawning the render task — bad arguments and a missing input keep
/// the immediate `Err` contract; only the actual render is deferred.
pub fn precheck(bit_depth: Option<u8>, input_path: &str) -> Result<()> {
    let bit_depth = bit_depth.unwrap_or(DEFAULT_BIT_DEPTH);
    if !matches!(bit_depth, 16 | 24 | 32) {
        return Err(anyhow!("bit_depth must be 16|24|32 (got {bit_depth})"));
    }
    let input = PathBuf::from(input_path);
    if !input.exists() {
        return Err(anyhow!("input WAV not found: {input:?}"));
    }
    Ok(())
}

/// Run an offline render for a `Command::RenderChain` payload.
///
/// `bit_depth` is validated up-front (only 16/24/32 are valid output
/// formats) — bad values surface as an `anyhow::Error` before the
/// renderer touches the filesystem, so no temp file is created.
///
/// On success returns [`Event::RenderCompleted`] carrying the absolute
/// output path plus the rendered WAV's key audio metadata (duration in
/// seconds derived from the engine sample rate, sample-rate echo, bit
/// depth echo).
#[allow(clippy::too_many_arguments)]
pub fn run(
    chain_path: String,
    input_path: String,
    output_path: String,
    start_s: Option<f32>,
    end_s: Option<f32>,
    sample_rate_hz: Option<u32>,
    block_size: Option<u32>,
    bit_depth: Option<u8>,
    tail_ms: Option<u32>,
) -> Result<Event> {
    let bit_depth = bit_depth.unwrap_or(DEFAULT_BIT_DEPTH);
    if !matches!(bit_depth, 16 | 24 | 32) {
        return Err(anyhow!("bit_depth must be 16|24|32 (got {bit_depth})"));
    }

    let args = RenderArgs {
        chain: PathBuf::from(chain_path),
        input: PathBuf::from(input_path),
        output: PathBuf::from(&output_path),
        start_s,
        end_s,
        // The Command path is file-mode only — `adapter-render` keys on
        // `args.input.exists()` to decide mode, so leaving these `None`
        // and demanding the input WAV already exists is what gates out
        // any accidental capture from inside the dispatcher.
        duration_s: None,
        input_device: None,
        sample_rate_hz: sample_rate_hz.unwrap_or(DEFAULT_SAMPLE_RATE_HZ),
        block_size: block_size.map(|v| v as usize).unwrap_or(DEFAULT_BLOCK_SIZE),
        bit_depth,
        tail_ms: tail_ms.unwrap_or(DEFAULT_TAIL_MS),
    };

    let summary: RenderSummary = render(&args).map_err(|e| match e {
        RenderError::InvalidArgs(msg) => anyhow!("render args: {msg}"),
        other => anyhow!(other.to_string()),
    })?;

    let duration_seconds = summary.frames_written as f64 / summary.sample_rate_hz as f64;
    Ok(Event::RenderCompleted {
        output_path,
        duration_seconds,
        sample_rate: summary.sample_rate_hz,
        bit_depth,
    })
}
