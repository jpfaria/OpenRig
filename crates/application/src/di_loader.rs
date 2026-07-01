//! Off-thread DI loop preload: decode + resample + loop-crossfade into
//! an `Arc<DiLoop>` ready for lock-free audio-thread reads.
//!
//! The audio thread NEVER touches this module. `load_di_loop` decodes a WAV
//! file (or a bundled asset by id) entirely on the calling thread, which must
//! be a non-audio thread (command side-effect or background task).
//!
//! # Layering
//! - `engine` stays IO-free: `DiLoop::from_samples` only does math.
//! - File decode uses `adapter_render::wav::read_wav` (existing `hound`-based
//!   reader, already a dependency of this crate via `adapter-render`).
//! - Bundled asset resolution uses `infra_filesystem::detect_data_root()`
//!   (same resolver block/IR assets use — see `local_dispatcher_plugin_catalog`).
//! - No new heavy dependencies added.

use std::path::PathBuf;
use std::sync::Arc;

use adapter_render::wav::read_wav;
use engine::DiPcm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where a DI loop comes from.
///
/// Derives `Serialize + Deserialize + JsonSchema` so it can be embedded in a
/// [`crate::command::Command`] variant and exposed as an MCP/gRPC tool
/// argument via [`crate::command_schema`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum DiLoopSource {
    /// A bundled loop whose file stem lives under `<assets-dir>/di-loops/`.
    /// Example: `"stratocaster-bridge"` resolves to
    /// `<data-root>/assets/di-loops/stratocaster-bridge.wav`.
    Bundled(String),
    /// An arbitrary file path chosen by the user.
    File(PathBuf),
}

/// Decode a DI source into an `Arc<DiPcm>` (the un-resampled source PCM).
///
/// Runs **off** the audio thread. The resample-to-rate + loop crossfade now
/// happen at ARM time, per output stream (#749): the arming path owns each
/// runtime's rate, so a multi-rate rig plays every output at true speed
/// instead of stretching a single `engine_sr` buffer. Returns `Err` with a
/// user-facing message on failure — never panics, never returns `Ok` with
/// silent/empty audio to mask an error.
pub fn load_di_loop(source: &DiLoopSource) -> Result<Arc<DiPcm>, String> {
    let path = resolve_path(source)?;
    let wav = read_wav(&path).map_err(|e| format!("DI loop read error for {path:?}: {e}"))?;

    if wav.samples.is_empty() {
        return Err(format!(
            "DI loop file {path:?} contains no samples"
        ));
    }

    Ok(Arc::new(DiPcm::new(
        wav.samples,
        wav.sample_rate_hz,
        wav.channels as usize,
    )))
}

/// Resolve a [`DiLoopSource`] to an absolute file path.
pub(crate) fn resolve_path(source: &DiLoopSource) -> Result<PathBuf, String> {
    match source {
        DiLoopSource::File(p) => Ok(p.clone()),
        DiLoopSource::Bundled(id) => {
            let root = infra_filesystem::detect_data_root();
            let path = root.join("assets").join("di-loops").join(format!("{id}.wav"));
            if !path.exists() {
                return Err(format!(
                    "bundled DI loop '{id}' not found at {path:?}"
                ));
            }
            Ok(path)
        }
    }
}
