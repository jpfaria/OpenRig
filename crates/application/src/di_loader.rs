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
use engine::DiLoop;

/// The default seam crossfade length in frames (≈ 10 ms at 48 kHz).
/// Passed to `DiLoop::from_samples` to hide the wrap click.
pub const DI_LOOP_XFADE_FRAMES: usize = 480;

/// Where a DI loop comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum DiLoopSource {
    /// A bundled loop whose file stem lives under `<assets-dir>/di-loops/`.
    /// Example: `"stratocaster-bridge"` resolves to
    /// `<data-root>/assets/di-loops/stratocaster-bridge.wav`.
    Bundled(String),
    /// An arbitrary file path chosen by the user.
    File(PathBuf),
}

/// Decode + resample + loop-crossfade a DI source into an `Arc<DiLoop>`.
///
/// Runs **off** the audio thread. Returns `Err` with a user-facing message on
/// failure — never panics, never returns `Ok` with silent/empty audio to mask
/// an error.
pub fn load_di_loop(source: &DiLoopSource, engine_sr: u32) -> Result<Arc<DiLoop>, String> {
    let path = resolve_path(source)?;
    let wav = read_wav(&path).map_err(|e| format!("DI loop read error for {path:?}: {e}"))?;

    if wav.samples.is_empty() {
        return Err(format!(
            "DI loop file {path:?} contains no samples"
        ));
    }

    let di = DiLoop::from_samples(
        &wav.samples,
        wav.sample_rate_hz,
        wav.channels as usize,
        engine_sr,
        DI_LOOP_XFADE_FRAMES,
    );
    Ok(Arc::new(di))
}

/// Resolve a [`DiLoopSource`] to an absolute file path.
fn resolve_path(source: &DiLoopSource) -> Result<PathBuf, String> {
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
