//! Offline render adapter for OpenRig.
//!
//! Drives the engine through a `.openrig` project headlessly, reading an input
//! WAV and writing an output WAV. No audio device, no GUI, no MIDI, no MCP.
//! Same `engine` block processors as live mode — deterministic.
//!
//! The DSP path is delegated to `engine::offline::render_chain`, which reuses
//! the same `RuntimeProcessor::process_buffer` as the realtime callback. The
//! adapter wraps that with project loading, WAV I/O, tail padding, and an
//! atomic output write.

pub mod cli;
pub mod wav;

use std::path::PathBuf;

use crate::cli::RenderArgs;
use crate::wav::{interleaved_to_stereo_frames, read_wav, write_wav_stereo, BitDepth};

/// Errors raised by [`render`].
#[derive(Debug)]
pub enum RenderError {
    ProjectLoad(anyhow::Error),
    InputRead(wav::WavError),
    OutputWrite(wav::WavError),
    InvalidArgs(String),
    /// The project has no buildable chain (e.g. legacy YAML with zero chains).
    NoChain,
    /// The named `--chain` argument did not match any chain in the project.
    ChainNotFound(String),
    /// The engine refused to build a runtime for the chosen chain.
    EngineBuild(anyhow::Error),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProjectLoad(e) => write!(f, "failed to load project: {e}"),
            Self::InputRead(e) => write!(f, "failed to read input wav: {e}"),
            Self::OutputWrite(e) => write!(f, "failed to write output wav: {e}"),
            Self::InvalidArgs(msg) => write!(f, "invalid render args: {msg}"),
            Self::NoChain => write!(f, "project has no chain to render"),
            Self::ChainNotFound(name) => write!(f, "chain not found in project: {name}"),
            Self::EngineBuild(e) => write!(f, "engine failed to build chain runtime: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// Summary of a successful render run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSummary {
    pub frames_written: u64,
    pub sample_rate_hz: u32,
    pub output: PathBuf,
}

/// Drive a project through an offline render and write the output WAV.
///
/// Atomic-on-failure: the output WAV is written to a sibling `<output>.tmp`
/// path first and only renamed into place once the render succeeds. A
/// failure mid-pipeline leaves no partial `<output>` file behind.
pub fn render(args: &RenderArgs) -> Result<RenderSummary, RenderError> {
    let bit_depth = bit_depth_from_arg(args.bit_depth)?;

    // 1. Load + validate the project.
    let project = infra_yaml::load_project_any(&args.project).map_err(RenderError::ProjectLoad)?;
    let chains = engine::rig_runtime::rig_to_chains(&project);
    let chain = pick_chain(&chains, args.chain.as_deref())?;

    // 2. Read input WAV and normalize to stereo frames.
    let input = read_wav(&args.input).map_err(RenderError::InputRead)?;
    let input_frames = interleaved_to_stereo_frames(&input.samples, input.channels);
    let tail_frames = (u64::from(args.tail_ms) * u64::from(args.sample_rate_hz) / 1000) as usize;

    // 3. Drive the chain offline through the engine — same processors as
    //    realtime, just supplied with a buffer-based driver instead of cpal.
    let output_frames = engine::offline::render_chain(
        chain,
        args.sample_rate_hz as f32,
        &input_frames,
        args.block_size,
        tail_frames,
    )
    .map_err(RenderError::EngineBuild)?;

    // 4. Atomic write: temp file + rename.
    let tmp = tmp_output_path(&args.output);
    write_wav_stereo(&tmp, &output_frames, args.sample_rate_hz, bit_depth)
        .map_err(RenderError::OutputWrite)?;
    std::fs::rename(&tmp, &args.output)
        .map_err(|e| RenderError::OutputWrite(wav::WavError::Io(e)))?;

    Ok(RenderSummary {
        frames_written: output_frames.len() as u64,
        sample_rate_hz: args.sample_rate_hz,
        output: args.output.clone(),
    })
}

fn pick_chain<'a>(
    chains: &'a [project::chain::Chain],
    requested: Option<&str>,
) -> Result<&'a project::chain::Chain, RenderError> {
    if chains.is_empty() {
        return Err(RenderError::NoChain);
    }
    match requested {
        None => Ok(&chains[0]),
        Some(name) => chains
            .iter()
            .find(|c| c.id.0 == name || c.description.as_deref() == Some(name))
            .ok_or_else(|| RenderError::ChainNotFound(name.to_string())),
    }
}

fn bit_depth_from_arg(bit_depth: u8) -> Result<BitDepth, RenderError> {
    match bit_depth {
        16 => Ok(BitDepth::Bits16),
        24 => Ok(BitDepth::Bits24),
        32 => Ok(BitDepth::Bits32Float),
        other => Err(RenderError::InvalidArgs(format!(
            "bit_depth must be 16|24|32 (got {other})"
        ))),
    }
}

fn tmp_output_path(output: &std::path::Path) -> PathBuf {
    let mut name = output.file_name().map(|n| n.to_owned()).unwrap_or_default();
    name.push(".tmp");
    output.with_file_name(name)
}
