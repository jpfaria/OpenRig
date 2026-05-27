//! CLI argument parsing for `openrig-render`.
//!
//! Hand-rolled — keeps the headless binary dep-light. The CLI surface is
//! intentionally small: a chain (preset YAML), an input wav (file source
//! OR live-capture cache path), an output wav. Optional slice flags
//! (`--start`/`--end`) trim the input in file mode; optional capture
//! flags (`--duration`/`--input-device`) bring up cpal when the input
//! path doesn't exist yet.

use std::path::PathBuf;

/// Parsed `openrig-render` invocation. Field defaults match
/// `docs/render.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderArgs {
    /// Path to the chain/preset YAML to apply (`presets/clean.yaml` style).
    pub chain: PathBuf,
    /// Input wav. If the path exists, it's the source (file mode). If
    /// it doesn't exist, the renderer captures from `input_device` for
    /// `duration_s` seconds and saves the dry capture here (live mode).
    pub input: PathBuf,
    /// Where to write the processed wav.
    pub output: PathBuf,
    /// Optional start of the input slice (seconds). File mode only.
    pub start_s: Option<f32>,
    /// Optional end of the input slice (seconds). File mode only.
    pub end_s: Option<f32>,
    /// Duration of the live capture in seconds. Required when `input`
    /// path doesn't exist; ignored otherwise.
    pub duration_s: Option<f32>,
    /// cpal input device name (substring match). `None` → default device.
    pub input_device: Option<String>,
    /// Engine sample rate.
    pub sample_rate_hz: u32,
    /// Inner process block size.
    pub block_size: usize,
    /// Output WAV bit depth (16, 24, or 32-float).
    pub bit_depth: u8,
    /// Extra silence appended after the input to capture reverb/delay tails.
    pub tail_ms: u32,
}

/// Errors raised while parsing the argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderArgsError {
    MissingRequired(String),
    MissingValue(String),
    UnknownFlag(String),
    InvalidValue {
        flag: String,
        value: String,
        reason: String,
    },
}

impl std::fmt::Display for RenderArgsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRequired(flag) => write!(f, "missing required flag: {flag}"),
            Self::MissingValue(flag) => write!(f, "flag {flag} expects a value"),
            Self::UnknownFlag(flag) => write!(f, "unknown flag: {flag}"),
            Self::InvalidValue {
                flag,
                value,
                reason,
            } => write!(f, "invalid value for {flag} ({value}): {reason}"),
        }
    }
}

impl std::error::Error for RenderArgsError {}

/// Parse the argv of `openrig-render`. `args[0]` is the binary name.
pub fn parse_render_args(args: &[String]) -> Result<RenderArgs, RenderArgsError> {
    let mut chain: Option<PathBuf> = None;
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut start_s: Option<f32> = None;
    let mut end_s: Option<f32> = None;
    let mut duration_s: Option<f32> = None;
    let mut input_device: Option<String> = None;
    let mut sample_rate_hz: u32 = 48_000;
    let mut block_size: usize = 256;
    let mut bit_depth: u8 = 24;
    let mut tail_ms: u32 = 2_000;

    let mut i = 1;
    while i < args.len() {
        let flag = &args[i];
        let value = args
            .get(i + 1)
            .ok_or_else(|| RenderArgsError::MissingValue(flag.clone()))?;

        match flag.as_str() {
            "--chain" => chain = Some(PathBuf::from(value)),
            "--input" => input = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            "--start" => start_s = Some(parse_f32(flag, value)?),
            "--end" => end_s = Some(parse_f32(flag, value)?),
            "--duration" => duration_s = Some(parse_f32(flag, value)?),
            "--input-device" => input_device = Some(value.clone()),
            "--sample-rate" => sample_rate_hz = parse_u32(flag, value)?,
            "--block-size" => block_size = parse_usize(flag, value)?,
            "--bit-depth" => {
                let n = parse_u8(flag, value)?;
                if !matches!(n, 16 | 24 | 32) {
                    return Err(RenderArgsError::InvalidValue {
                        flag: flag.clone(),
                        value: value.clone(),
                        reason: "must be one of 16, 24, 32".into(),
                    });
                }
                bit_depth = n;
            }
            "--tail-ms" => tail_ms = parse_u32(flag, value)?,
            other => return Err(RenderArgsError::UnknownFlag(other.to_string())),
        }

        i += 2;
    }

    Ok(RenderArgs {
        chain: chain.ok_or_else(|| RenderArgsError::MissingRequired("--chain".into()))?,
        input: input.ok_or_else(|| RenderArgsError::MissingRequired("--input".into()))?,
        output: output.ok_or_else(|| RenderArgsError::MissingRequired("--output".into()))?,
        start_s,
        end_s,
        duration_s,
        input_device,
        sample_rate_hz,
        block_size,
        bit_depth,
        tail_ms,
    })
}

fn parse_u32(flag: &str, value: &str) -> Result<u32, RenderArgsError> {
    value
        .parse::<u32>()
        .map_err(|e| RenderArgsError::InvalidValue {
            flag: flag.into(),
            value: value.into(),
            reason: e.to_string(),
        })
}

fn parse_usize(flag: &str, value: &str) -> Result<usize, RenderArgsError> {
    value
        .parse::<usize>()
        .map_err(|e| RenderArgsError::InvalidValue {
            flag: flag.into(),
            value: value.into(),
            reason: e.to_string(),
        })
}

fn parse_u8(flag: &str, value: &str) -> Result<u8, RenderArgsError> {
    value
        .parse::<u8>()
        .map_err(|e| RenderArgsError::InvalidValue {
            flag: flag.into(),
            value: value.into(),
            reason: e.to_string(),
        })
}

fn parse_f32(flag: &str, value: &str) -> Result<f32, RenderArgsError> {
    value
        .parse::<f32>()
        .map_err(|e| RenderArgsError::InvalidValue {
            flag: flag.into(),
            value: value.into(),
            reason: e.to_string(),
        })
}
