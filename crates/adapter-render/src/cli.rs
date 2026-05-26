//! CLI argument parsing for `openrig --render` / `openrig-render`.
//!
//! Hand-rolled to avoid pulling clap into the headless render binary. The
//! surface is small and exact-match — keeping deps minimal helps the
//! cross-platform headless build (issue #552).

use std::path::PathBuf;

/// Parsed `openrig-render` invocation. Field defaults match the documented
/// CLI surface in `docs/cli.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderArgs {
    pub project: PathBuf,
    pub input: PathBuf,
    pub output: PathBuf,
    pub chain: Option<String>,
    pub sample_rate_hz: u32,
    pub block_size: usize,
    pub bit_depth: u8,
    pub tail_ms: u32,
}

/// Errors that can arise while parsing the `openrig-render` argv.
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
    /// `--render` was combined with a flag that requires the live rig
    /// (e.g. `--mcp`, `--midi`, or a positional project path).
    MutuallyExclusive(String),
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
            } => {
                write!(f, "invalid value for {flag} ({value}): {reason}")
            }
            Self::MutuallyExclusive(flag) => {
                write!(f, "--render is mutually exclusive with {flag}")
            }
        }
    }
}

impl std::error::Error for RenderArgsError {}

/// Parse the argv of `openrig-render` (or the `--render` subset of `openrig`).
///
/// `args[0]` is treated as the binary name and ignored.
pub fn parse_render_args(args: &[String]) -> Result<RenderArgs, RenderArgsError> {
    let mut project: Option<PathBuf> = None;
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut chain: Option<String> = None;
    let mut sample_rate_hz: u32 = 48_000;
    let mut block_size: usize = 256;
    let mut bit_depth: u8 = 24;
    let mut tail_ms: u32 = 2_000;

    let mut i = 1; // skip argv[0] (binary name)
    while i < args.len() {
        let flag = &args[i];

        // Boolean / mutually-exclusive flags — no value to consume.
        match flag.as_str() {
            "--mcp" | "--midi" => {
                return Err(RenderArgsError::MutuallyExclusive(flag.clone()));
            }
            // The `--render` flag itself is permitted but carries no value;
            // it just marks render mode (consumed by the outer `openrig`
            // dispatcher; harmless if it ends up here).
            "--render" => {
                i += 1;
                continue;
            }
            _ => {}
        }

        // Value-bearing flags consume the next argv slot.
        let value = match args.get(i + 1) {
            Some(v) if !v.starts_with("--") => v,
            _ => return Err(RenderArgsError::MissingValue(flag.clone())),
        };

        match flag.as_str() {
            "--project" => project = Some(PathBuf::from(value)),
            "--input" => input = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            "--chain" => chain = Some(value.clone()),
            "--sample-rate" => {
                sample_rate_hz = parse_u32(flag, value)?;
            }
            "--block-size" => {
                block_size = parse_usize(flag, value)?;
            }
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
            "--tail-ms" => {
                tail_ms = parse_u32(flag, value)?;
            }
            other => return Err(RenderArgsError::UnknownFlag(other.to_string())),
        }

        i += 2;
    }

    Ok(RenderArgs {
        project: project.ok_or_else(|| RenderArgsError::MissingRequired("--project".into()))?,
        input: input.ok_or_else(|| RenderArgsError::MissingRequired("--input".into()))?,
        output: output.ok_or_else(|| RenderArgsError::MissingRequired("--output".into()))?,
        chain,
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
