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

/// Quality preset: balances SDR vs wall-clock per song.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Quality {
    /// Single-model htdemucs_6s, shifts=1. Fastest.
    Fast,
    /// Ensemble htdemucs_ft (4 stems, highest SDR) + htdemucs_6s
    /// (steals Guitar + Piano). 2 shifts each. ~5x slower than Fast.
    Best,
}

impl Quality {
    pub(crate) fn from_env() -> Self {
        match std::env::var("OPENRIG_STEMS_QUALITY")
            .ok()
            .as_deref()
            .map(str::trim)
        {
            Some("fast") => Self::Fast,
            _ => Self::Best,
        }
    }
}

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

/// Run one Demucs invocation and return the per-stem buffers in
/// canonical (filename-alphabetical-mapped-to-StemKind) order.
fn run_one(
    bin: &Path,
    source: &Path,
    model: &str,
    shifts: u32,
) -> Result<Vec<Vec<f32>>, StemError> {
    let tmp = tempdir().map_err(|err| StemError::Inference {
        reason: format!("tempdir: {err}"),
    })?;

    let status = Command::new(bin)
        .arg("-n")
        .arg(model)
        .arg("--shifts")
        .arg(shifts.to_string())
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
            reason: format!("demucs {model} exited with status {status:?}"),
        });
    }

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

/// Best-quality stem separation: ensemble of `htdemucs_ft` (4 stems,
/// highest open-source SDR) + `htdemucs_6s` (steals Guitar + Piano),
/// each with 2 shifts averaging for an extra ~0.5 dB SDR.
///
/// Result is always 6 stems in canonical order: `[drums, bass, vocals,
/// other, guitar, piano]`. drums/bass/vocals/other come from `_ft`;
/// guitar/piano come from `_6s`.
pub(crate) fn separate_via_demucs_cli(
    source: &Path,
    quality: Quality,
) -> Result<Vec<Vec<f32>>, StemError> {
    let bin = locate_demucs_binary().ok_or_else(|| StemError::Inference {
        reason: "demucs binary not found (set DEMUCS_BIN or install via .venv-tracks)".to_string(),
    })?;

    match quality {
        Quality::Fast => run_one(&bin, source, "htdemucs_6s", 1),
        Quality::Best => {
            let four = run_one(&bin, source, "htdemucs_ft", 2)?;
            let six = run_one(&bin, source, "htdemucs_6s", 2)?;
            // canonical_filename_order is [drums, bass, vocals, other]
            // for `_ft` and [drums, bass, vocals, other, guitar, piano]
            // for `_6s`. Take 0..4 from `_ft` (highest SDR) and 4..6
            // from `_6s`.
            if four.len() != 4 || six.len() != 6 {
                return Err(StemError::Inference {
                    reason: format!(
                        "ensemble shape mismatch: ft={} stems, 6s={} stems (want 4 and 6)",
                        four.len(),
                        six.len()
                    ),
                });
            }
            let mut merged = four;
            merged.push(six[4].clone());
            merged.push(six[5].clone());
            Ok(merged)
        }
    }
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
        // `htdemucs`, `htdemucs_ft`, anything else 4-stem.
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
