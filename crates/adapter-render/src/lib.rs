//! Offline render adapter for OpenRig.
//!
//! Drives the engine through a `.openrig` project headlessly, reading an input
//! WAV and writing an output WAV. No audio device, no GUI, no MIDI, no MCP.
//! Same `engine.process_block()` as live mode — deterministic.
//!
//! Current state (issue #552):
//!
//! * Phases P1–P3 ship the CLI surface and the WAV I/O helpers.
//! * Phase P4 (this module) is intentionally a **passthrough shell**: it
//!   loads + validates the project, reads the input WAV, writes the output
//!   WAV padded with `--tail-ms` of silence, and atomically renames the
//!   final file into place. It does NOT yet route samples through the
//!   engine — that is P4b, which requires either an offline-mode runtime
//!   in `crates/engine` or a stripped-down DSP walker bypassing the
//!   device-bound input/output blocks. Both options need a design call
//!   captured in the umbrella spec before implementation.
//!
//! The shell is still useful: the CLI surface, error mapping, atomic write
//! and `--tail-ms` handling are exercised end-to-end, and the analyzer
//! pipeline (issue OpenRig-claude#8) can already round-trip files through
//! `openrig-render` to validate its own JSON contracts.

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
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProjectLoad(e) => write!(f, "failed to load project: {e}"),
            Self::InputRead(e) => write!(f, "failed to read input wav: {e}"),
            Self::OutputWrite(e) => write!(f, "failed to write output wav: {e}"),
            Self::InvalidArgs(msg) => write!(f, "invalid render args: {msg}"),
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

    // 1. Load + validate the project (errors propagated even though we don't
    //    instantiate the engine yet — the consumer pipeline must still be
    //    able to fail fast on a broken project file).
    infra_yaml::load_project_any(&args.project).map_err(RenderError::ProjectLoad)?;

    // 2. Read input WAV and normalize to stereo frames.
    let input = read_wav(&args.input).map_err(RenderError::InputRead)?;
    let mut frames = interleaved_to_stereo_frames(&input.samples, input.channels);

    // 3. Append `--tail-ms` of silence so reverb/delay tails are captured
    //    once the engine integration lands.
    let tail_frames = (u64::from(args.tail_ms) * u64::from(args.sample_rate_hz) / 1000) as usize;
    frames.extend(std::iter::repeat_n([0.0_f32, 0.0_f32], tail_frames));

    // 4. Atomic write: temp file + rename.
    let tmp = tmp_output_path(&args.output);
    write_wav_stereo(&tmp, &frames, args.sample_rate_hz, bit_depth)
        .map_err(RenderError::OutputWrite)?;
    std::fs::rename(&tmp, &args.output)
        .map_err(|e| RenderError::OutputWrite(wav::WavError::Io(e)))?;

    Ok(RenderSummary {
        frames_written: frames.len() as u64,
        sample_rate_hz: args.sample_rate_hz,
        output: args.output.clone(),
    })
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
