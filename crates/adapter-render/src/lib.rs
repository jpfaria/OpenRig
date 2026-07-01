//! Offline render adapter for OpenRig.
//!
//! Drives the engine through a chain/preset YAML headlessly, reading an
//! input WAV (or capturing live) and writing an output WAV. No GUI, no
//! MIDI, no MCP. Same `engine` block processors as live mode — deterministic.
//!
//! Modes (decided by whether `args.input` already exists on disk):
//!
//! * **File mode** — `args.input` exists → read it, optionally slice with
//!   `args.start_s` / `args.end_s`, push through the chain.
//! * **Live-capture mode** — `args.input` does NOT exist and
//!   `args.duration_s` is set → capture from `args.input_device` (or the
//!   default cpal input device) for that many seconds, save the dry
//!   capture to `args.input` (so subsequent runs reuse it via file mode
//!   without making you play again), then push through the chain.

pub mod capture;
pub mod cli;
pub mod wav;

use std::path::PathBuf;

use project::block::AudioBlock;
use project::chain::Chain;

use crate::cli::RenderArgs;
use crate::wav::{interleaved_to_stereo_frames, read_wav, write_wav_stereo, BitDepth};

/// Errors raised by [`render`].
#[derive(Debug)]
pub enum RenderError {
    ChainLoad(anyhow::Error),
    InputRead(wav::WavError),
    OutputWrite(wav::WavError),
    InvalidArgs(String),
    /// The engine refused to build a runtime for the chain.
    EngineBuild(anyhow::Error),
    /// One or more blocks in the chain could not be built into runtime
    /// processors and would have been silently bypassed. Refusing to
    /// claim success — otherwise different presets render to identical
    /// bytes because every failing block disappears from the signal path
    /// (issue #574). Each entry is `(block_id, effect_type, model, error)`.
    BlocksFailed(Vec<engine::offline::FaultedBlock>),
    /// Live capture was requested but `--duration` was not supplied (or
    /// cpal could not open the requested input device).
    Capture(anyhow::Error),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChainLoad(e) => write!(f, "failed to load chain: {e}"),
            Self::InputRead(e) => write!(f, "failed to read input wav: {e}"),
            Self::OutputWrite(e) => write!(f, "failed to write output wav: {e}"),
            Self::InvalidArgs(msg) => write!(f, "invalid render args: {msg}"),
            Self::EngineBuild(e) => write!(f, "engine failed to build chain runtime: {e}"),
            Self::BlocksFailed(faulted) => {
                writeln!(
                    f,
                    "{} block(s) in the chain failed to build and would have been silently bypassed:",
                    faulted.len()
                )?;
                for fb in faulted {
                    writeln!(
                        f,
                        "  - block '{}' ({}/{}): {}",
                        fb.block_id, fb.effect_type, fb.model, fb.error
                    )?;
                }
                write!(
                    f,
                    "refusing to write a WAV that would be missing those blocks' contribution"
                )
            }
            Self::Capture(e) => write!(f, "live capture failed: {e}"),
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
    /// True if the input WAV was created by this run (live capture); false
    /// if it was already on disk and used as a file source.
    pub captured_input: bool,
}

/// Drive a chain through an offline render and write the output WAV.
///
/// Atomic-on-failure: the output WAV is written to a sibling `<output>.tmp`
/// path first and only renamed into place once the render succeeds.
pub fn render(args: &RenderArgs) -> Result<RenderSummary, RenderError> {
    let bit_depth = bit_depth_from_arg(args.bit_depth)?;

    // 1. Load the chain/preset.
    let chain = load_chain(&args.chain)?;

    // 2. Ensure we have an input WAV — file mode or live capture.
    let captured_input = ensure_input_wav(args)?;

    // 3. Read input WAV and normalize to stereo frames.
    let input = read_wav(&args.input).map_err(RenderError::InputRead)?;
    let mut input_frames = interleaved_to_stereo_frames(&input.samples, input.channels);
    apply_slice(&mut input_frames, input.sample_rate_hz, args)?;

    let tail_frames = (u64::from(args.tail_ms) * u64::from(args.sample_rate_hz) / 1000) as usize;

    // 4. Drive the chain offline through the engine.
    let outcome = engine::offline::render_chain(
        &chain,
        args.sample_rate_hz as f32,
        &input_frames,
        args.block_size,
        tail_frames,
    )
    .map_err(RenderError::EngineBuild)?;

    // Issue #574: the engine returns a best-effort render even when some
    // blocks could not be built (the GUI relies on that to keep running
    // with a partial chain). The CLI must NOT inherit that policy — a
    // WAV that silently drops the user's amp/cab/effect is worse than no
    // WAV. Fail loud before writing anything.
    if !outcome.faulted_blocks.is_empty() {
        return Err(RenderError::BlocksFailed(outcome.faulted_blocks));
    }

    // 5. Atomic write: temp file + rename.
    let tmp = tmp_output_path(&args.output);
    write_wav_stereo(&tmp, &outcome.samples, args.sample_rate_hz, bit_depth)
        .map_err(RenderError::OutputWrite)?;
    std::fs::rename(&tmp, &args.output)
        .map_err(|e| RenderError::OutputWrite(wav::WavError::Io(e)))?;

    Ok(RenderSummary {
        frames_written: outcome.samples.len() as u64,
        sample_rate_hz: args.sample_rate_hz,
        output: args.output.clone(),
        captured_input,
    })
}

fn load_chain(path: &std::path::Path) -> Result<Chain, RenderError> {
    let preset = infra_yaml::load_chain_preset_file(path).map_err(RenderError::ChainLoad)?;
    Ok(synthesize_chain(preset.blocks))
}

/// Wrap a flat list of blocks (`presets/*.yaml` shape) into a minimal
/// `Chain` so it can be fed to the engine's offline driver. The chain has
/// no input/output blocks — the offline driver supplies the bus directly,
/// matching `engine::offline::render_chain`'s contract.
fn synthesize_chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: domain::ids::ChainId("render".to_string()),
        description: Some("openrig-render".to_string()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
    }
}

/// Returns `true` iff the input WAV was created by a live capture in
/// this call; `false` if it was already on disk.
fn ensure_input_wav(args: &RenderArgs) -> Result<bool, RenderError> {
    if args.input.exists() {
        return Ok(false);
    }
    let duration_s = args.duration_s.ok_or_else(|| {
        RenderError::InvalidArgs(format!(
            "input wav {} does not exist; pass --duration to capture from interface",
            args.input.display()
        ))
    })?;
    if duration_s <= 0.0 {
        return Err(RenderError::InvalidArgs(format!(
            "--duration must be > 0 (got {duration_s})"
        )));
    }
    capture::capture_to_wav(
        &args.input,
        args.input_device.as_deref(),
        duration_s,
        args.sample_rate_hz,
    )
    .map_err(RenderError::Capture)?;
    Ok(true)
}

fn apply_slice(
    frames: &mut Vec<[f32; 2]>,
    input_sr: u32,
    args: &RenderArgs,
) -> Result<(), RenderError> {
    if args.start_s.is_none() && args.end_s.is_none() {
        return Ok(());
    }
    let len = frames.len();
    let start = args
        .start_s
        .map(|s| seconds_to_frame(s, input_sr).min(len))
        .unwrap_or(0);
    let end = args
        .end_s
        .map(|e| seconds_to_frame(e, input_sr).min(len))
        .unwrap_or(len);
    if start >= end {
        return Err(RenderError::InvalidArgs(format!(
            "--start ({start} frame) >= --end ({end} frame); input only has {len} frames"
        )));
    }
    frames.drain(end..);
    frames.drain(..start);
    Ok(())
}

fn seconds_to_frame(seconds: f32, sample_rate_hz: u32) -> usize {
    (seconds.max(0.0) * sample_rate_hz as f32).round() as usize
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
