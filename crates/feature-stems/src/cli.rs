//! Shell out to the `demucs` Python CLI for real source separation.
//!
//! The ONNX export path for htdemucs hits a hard wall in vanilla
//! `torch.onnx.export` (STFT-with-complex isn't supported by the
//! legacy exporter and the dynamo exporter blows up on Demucs's
//! data-dependent padding — Mixxx is doing a whole GSoC to fix it).
//!
//! Until that lands, the most reliable real-separation path is to
//! call the official `demucs` Python CLI directly. The Rust side
//! orchestrates a temp dir, parses the per-stem WAVs that the CLI
//! writes, and returns them as in-memory buffers. The user only has
//! to install the `demucs` package once (any venv works; we also
//! probe a project-local `.venv-tracks` to make the dev install
//! turnkey).
//!
//! Auto-detection (in order, first hit wins):
//!   1. `$DEMUCS_BIN` env var.
//!   2. `./.venv-tracks/bin/demucs` (matches the dev install script).
//!   3. `demucs` on `$PATH`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::decode_audio;
use crate::StemError;

/// Locate a usable `demucs` binary. Returns `None` when none of the
/// candidates exist, which the orchestrator treats as "fall back to
/// the stub separator".
pub(crate) fn locate_demucs_binary() -> Option<PathBuf> {
    if let Ok(env) = std::env::var("DEMUCS_BIN") {
        let p = PathBuf::from(env);
        if p.is_file() {
            return Some(p);
        }
    }
    let local = PathBuf::from(".venv-tracks/bin/demucs");
    if local.is_file() {
        return Some(local);
    }
    if let Some(found) = which_on_path("demucs") {
        return Some(found);
    }
    None
}

fn which_on_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Run `demucs -n <model> -o <tmp> --filename "{stem}.wav" <source>`
/// and decode each emitted stem back as interleaved stereo `f32`.
///
/// The exact stem set depends on the model: `htdemucs` → 4 stems,
/// `htdemucs_6s` → 6 stems. The returned vector is ordered the same
/// way the CLI writes them on disk (alphabetical) and then mapped
/// back to canonical order by [`StemKind::layout_for`] in the
/// orchestrator.
pub(crate) fn separate_via_demucs_cli(
    source: &Path,
    model: &str,
) -> Result<Vec<Vec<f32>>, StemError> {
    let bin = locate_demucs_binary().ok_or_else(|| StemError::Inference {
        reason: "demucs binary not found (set DEMUCS_BIN or install via .venv-tracks)".to_string(),
    })?;

    let tmp = tempdir().map_err(|err| StemError::Inference {
        reason: format!("tempdir: {err}"),
    })?;

    let status = Command::new(&bin)
        .arg("-n")
        .arg(model)
        .arg("--filename")
        .arg("{stem}.wav")
        .arg("--float32")
        .arg("-o")
        .arg(tmp.path())
        .arg(source)
        .status()
        .map_err(|err| StemError::Inference {
            reason: format!("demucs spawn `{}`: {err}", bin.display()),
        })?;
    if !status.success() {
        return Err(StemError::Inference {
            reason: format!("demucs exited with status {status:?}"),
        });
    }

    // With `--filename "{stem}.wav"` the CLI skips the per-track
    // subdirectory — outputs land directly under `<tmp>/<model>/`.
    let stems_dir = tmp.path().join(model);

    let order = canonical_filename_order(model);
    let mut stems = Vec::with_capacity(order.len());
    for filename in order {
        let path = stems_dir.join(filename);
        let decoded = decode_audio(&path)?;
        stems.push(decoded.samples);
    }
    Ok(stems)
}

/// Canonical stem-filename order for a given Demucs model name.
/// Matches `StemKind::layout_for` on the consumer side.
fn canonical_filename_order(model: &str) -> &'static [&'static str] {
    match model {
        "htdemucs_6s" => &[
            "drums.wav",
            "bass.wav",
            "vocals.wav",
            "other.wav",
            "guitar.wav",
            "piano.wav",
        ],
        _ => &["drums.wav", "bass.wav", "vocals.wav", "other.wav"],
    }
}

/// Tempdir helper that respects `$TMPDIR` and cleans up on Drop.
fn tempdir() -> std::io::Result<TempDir> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = base.join(format!("openrig-demucs-{pid}-{nanos}"));
    std::fs::create_dir_all(&dir)?;
    Ok(TempDir { path: dir })
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
