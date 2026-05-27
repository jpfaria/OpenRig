//! `render_chain` MCP tool — wraps the headless offline render shipped by
//! `adapter-render` (issue #552) so an MCP client can run a chain against a
//! WAV (or capture live) without shelling out to the `openrig-render` binary.
//!
//! Scope: read-only / offline; does NOT mutate the live rig or `State`. It is
//! therefore registered alongside the `Command`-derived tool catalog in
//! [`crate::server`] rather than going through `command_schema`.
//!
//! Error mapping mirrors the CLI exit codes (`docs/render.md`):
//!   * argument-level rejections (invalid `bit_depth`, malformed JSON args) →
//!     [`RenderChainError::InvalidParams`] → MCP `invalid_params`;
//!   * everything from [`adapter_render::render`] (chain load failures,
//!     missing input without `--duration`, engine error, IO error) →
//!     [`RenderChainError::RenderFailed`] → MCP `internal_error`.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{CallToolResult, Content, Tool};
use rmcp::ErrorData;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use adapter_render::cli::RenderArgs;
use adapter_render::render;

/// MCP tool name. Stable wire identifier — do not rename without bumping
/// the client contract.
pub const RENDER_CHAIN_TOOL_NAME: &str = "render_chain";

const DEFAULT_SAMPLE_RATE_HZ: u32 = 48_000;
const DEFAULT_BLOCK_SIZE: usize = 256;
const DEFAULT_BIT_DEPTH: u8 = 24;
const DEFAULT_TAIL_MS: u32 = 2_000;

/// Tool arguments. Mirrors the `openrig-render` CLI flags one-to-one.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RenderChainInput {
    /// Chain/preset YAML — same shape as `presets/clean.yaml`.
    pub chain_path: PathBuf,
    /// Input WAV. Existing file → file mode. Missing file + `duration_s` set
    /// → live capture mode (the dry capture is written to this path so
    /// subsequent calls reuse it).
    pub input_path: PathBuf,
    /// Output WAV path. Written atomically via `<path>.tmp` + rename.
    pub output_path: PathBuf,
    /// File mode only. Skip the first N seconds of the input WAV.
    #[serde(default)]
    pub start_s: Option<f32>,
    /// File mode only. Stop at N seconds of the input WAV.
    #[serde(default)]
    pub end_s: Option<f32>,
    /// Live-capture mode only. Capture from the input device for N seconds.
    #[serde(default)]
    pub duration_s: Option<f32>,
    /// Live-capture mode only. Substring match against cpal input device
    /// names; `None` → default input device.
    #[serde(default)]
    pub input_device: Option<String>,
    /// Engine sample rate. Default 48000 Hz.
    #[serde(default)]
    pub sample_rate_hz: Option<u32>,
    /// Internal process block size. Default 256 frames.
    #[serde(default)]
    pub block_size: Option<u32>,
    /// Output WAV sample format. `16`/`24` = signed PCM, `32` = 32-bit float.
    /// Default 24.
    #[serde(default)]
    pub bit_depth: Option<u8>,
    /// Extra silence appended after the input so reverb/delay tails are not
    /// truncated. Default 2000 ms.
    #[serde(default)]
    pub tail_ms: Option<u32>,
}

/// Tool result. Returned as JSON in the MCP `CallToolResult` body.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderChainOutput {
    pub output_path: String,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub bit_depth: u8,
    /// `"file"` when the input WAV already existed, `"live"` when it was
    /// captured during this call.
    pub mode: String,
}

/// Errors raised by [`render_chain`]. Split on the two MCP error families:
/// argument-level (`invalid_params`) vs. render-time (`internal_error`).
#[derive(Debug)]
pub enum RenderChainError {
    InvalidParams(String),
    RenderFailed(String),
}

impl std::fmt::Display for RenderChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidParams(msg) => write!(f, "invalid render_chain arguments: {msg}"),
            Self::RenderFailed(msg) => write!(f, "render_chain failed: {msg}"),
        }
    }
}

impl std::error::Error for RenderChainError {}

/// Run the headless offline render. Pure function — same call site as the
/// `openrig-render` binary. Output WAV is atomic-on-failure (written to
/// `<output>.tmp` and renamed once the engine returns), so a failed render
/// never leaves a half-written file behind.
pub fn render_chain(input: RenderChainInput) -> Result<RenderChainOutput, RenderChainError> {
    let bit_depth = input.bit_depth.unwrap_or(DEFAULT_BIT_DEPTH);
    if !matches!(bit_depth, 16 | 24 | 32) {
        return Err(RenderChainError::InvalidParams(format!(
            "bit_depth must be 16|24|32 (got {bit_depth})"
        )));
    }

    let mode = if input.input_path.exists() {
        "file"
    } else {
        "live"
    };

    let args = RenderArgs {
        chain: input.chain_path,
        input: input.input_path,
        output: input.output_path.clone(),
        start_s: input.start_s,
        end_s: input.end_s,
        duration_s: input.duration_s,
        input_device: input.input_device,
        sample_rate_hz: input.sample_rate_hz.unwrap_or(DEFAULT_SAMPLE_RATE_HZ),
        block_size: input
            .block_size
            .map(|v| v as usize)
            .unwrap_or(DEFAULT_BLOCK_SIZE),
        bit_depth,
        tail_ms: input.tail_ms.unwrap_or(DEFAULT_TAIL_MS),
    };

    let summary = render(&args).map_err(|e| RenderChainError::RenderFailed(e.to_string()))?;

    let duration_seconds = summary.frames_written as f64 / summary.sample_rate_hz as f64;
    Ok(RenderChainOutput {
        output_path: input.output_path.to_string_lossy().into_owned(),
        duration_seconds,
        sample_rate: summary.sample_rate_hz,
        bit_depth,
        mode: mode.to_string(),
    })
}

/// MCP `Tool` descriptor for [`render_chain`]. Registered alongside the
/// auto-derived Command tools by [`crate::server::OpenRigMcp::list_tools`].
pub fn tool() -> Tool {
    let schema = serde_json::to_value(schema_for!(RenderChainInput))
        .expect("RenderChainInput schema serializes");
    let obj: Map<String, Value> = schema.as_object().cloned().unwrap_or_default();
    Tool::new(
        RENDER_CHAIN_TOOL_NAME,
        "Offline render: apply a chain/preset YAML to an input WAV (or live capture) and write the processed result to an output WAV. Wraps `openrig-render`.",
        Arc::new(obj),
    )
}

/// Handle an MCP `call_tool` request whose name is [`RENDER_CHAIN_TOOL_NAME`].
/// Returns the JSON-encoded [`RenderChainOutput`] on success or an
/// [`ErrorData`] mapped from [`RenderChainError`].
pub fn handle(args: Value) -> Result<CallToolResult, ErrorData> {
    let input: RenderChainInput = serde_json::from_value(args).map_err(|e| {
        ErrorData::invalid_params(format!("render_chain: invalid arguments: {e}"), None)
    })?;
    match render_chain(input) {
        Ok(out) => {
            let json = serde_json::to_string(&out)
                .unwrap_or_else(|e| format!("<RenderChainOutput serialize error: {e}>"));
            Ok(CallToolResult::success(vec![Content::text(json)]))
        }
        Err(RenderChainError::InvalidParams(msg)) => Err(ErrorData::invalid_params(msg, None)),
        Err(RenderChainError::RenderFailed(msg)) => Err(ErrorData::internal_error(msg, None)),
    }
}
