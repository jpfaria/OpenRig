//! State-of-the-art stem separation via `mlx-audio-separator`
//! (Apple Silicon native via MLX) + a hand-tuned ensemble.
//!
//! The CLI exposes ~150 models. To beat Moises on per-stem SDR we
//! pick the leaders for each role and combine:
//!
//! - **Vocals**: MelBand Roformer Big Beta 5e — 12.4 dB SDR
//!   (htdemucs_ft does ~9.9 dB; +2.5 dB on the most-listened stem
//!   is night-and-day in practice).
//! - **Drums, Bass, Other**: htdemucs_ft — 10.0 / 12.0 / x dB,
//!   still the best 4-stem model after the vocals are pulled out.
//! - **Guitar, Piano**: htdemucs_6s — the only open model that
//!   gives them at all.
//!
//! The pipeline runs roformer first to extract a high-quality vocals
//! stem, then re-runs htdemucs_ft / htdemucs_6s on the FULL source
//! (not the instrumental — Demucs is already trained to handle the
//! whole mix and its drums/bass extraction is robust against
//! vocal bleed). We keep the roformer vocals and the demucs guitar
//! / piano / drums / bass / other.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::decode_audio;
use crate::StemError;

const VOCALS_MODEL: &str = "melband_roformer_big_beta5e.ckpt";
const FOUR_STEM_MODEL: &str = "htdemucs_ft.yaml";
const SIX_STEM_MODEL: &str = "htdemucs_6s.yaml";

/// Resolve the `mlx-audio-separator` binary, preferring the dev
/// venv that the conversion script creates.
pub(crate) fn locate_mlx_binary() -> Option<PathBuf> {
    if let Ok(env) = std::env::var("OPENRIG_MLX_SEPARATOR") {
        let p = PathBuf::from(env);
        if p.is_file() {
            return Some(p);
        }
    }
    let local = PathBuf::from(".venv-tracks/bin/mlx-audio-separator");
    if local.is_file() {
        return Some(local);
    }
    crate::cli::which("mlx-audio-separator")
}

/// Run a single model and return the per-stem buffers in the order
/// `expected_stems` lists them. The CLI writes files named
/// `<source-stem>_(<stem>)_<model>.wav` so we look those up by
/// suffix-matching.
fn run_model(
    bin: &Path,
    source: &Path,
    model: &str,
    expected_stems: &[&str],
) -> Result<Vec<Vec<f32>>, StemError> {
    let tmp = tempdir().map_err(|err| StemError::Inference {
        reason: format!("tempdir: {err}"),
    })?;
    let status = Command::new(bin)
        .arg(source)
        .arg("-m")
        .arg(model)
        .arg("--output_format")
        .arg("WAV")
        .arg("--output_dir")
        .arg(tmp.path())
        .status()
        .map_err(|err| StemError::Inference {
            reason: format!("mlx-audio-separator spawn `{}`: {err}", bin.display()),
        })?;
    if !status.success() {
        return Err(StemError::Inference {
            reason: format!("mlx-audio-separator {model} exited with status {status:?}"),
        });
    }

    let source_stem =
        source
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| StemError::Inference {
                reason: format!("cannot derive track stem from `{}`", source.display()),
            })?;

    let mut out = Vec::with_capacity(expected_stems.len());
    for stem_name in expected_stems {
        // The CLI uses parentheses around the stem and prepends the
        // source filename. Match by suffix `(<stem>)_` so the model
        // suffix can change without breaking us.
        let needle = format!("({stem_name})");
        let mut found: Option<PathBuf> = None;
        for entry in std::fs::read_dir(tmp.path()).map_err(|err| StemError::Inference {
            reason: format!("read mlx outputs: {err}"),
        })? {
            let entry = entry.map_err(|err| StemError::Inference {
                reason: format!("read mlx outputs: {err}"),
            })?;
            let p = entry.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.starts_with(source_stem) && name.contains(&needle) {
                found = Some(p);
                break;
            }
        }
        let path = found.ok_or_else(|| StemError::Inference {
            reason: format!(
                "mlx-audio-separator did not emit `{stem_name}` for model `{model}` in `{}`",
                tmp.path().display()
            ),
        })?;
        let decoded = decode_audio(&path)?;
        out.push(decoded.samples);
    }
    Ok(out)
}

/// Best-quality 6-stem ensemble via mlx-audio-separator. Always
/// returns 6 stems in the canonical order
/// `[drums, bass, vocals, other, guitar, piano]`.
pub(crate) fn separate_via_mlx_ensemble(source: &Path) -> Result<Vec<Vec<f32>>, StemError> {
    let bin = locate_mlx_binary().ok_or_else(|| StemError::Inference {
        reason: "mlx-audio-separator binary not found (install via .venv-tracks)".to_string(),
    })?;

    // 1. Roformer for vocals (and an instrumental we don't use).
    let roformer = run_model(&bin, source, VOCALS_MODEL, &["vocals"])?;

    // 2. htdemucs_ft for the standard 4 stems (we keep drums, bass,
    //    other and discard its vocals in favour of the roformer one).
    let four = run_model(&bin, source, FOUR_STEM_MODEL, &["drums", "bass", "other"])?;

    // 3. htdemucs_6s — the only model that gives Guitar and Piano.
    let six = run_model(&bin, source, SIX_STEM_MODEL, &["guitar", "piano"])?;

    if roformer.len() != 1 || four.len() != 3 || six.len() != 2 {
        return Err(StemError::Inference {
            reason: format!(
                "ensemble shape mismatch: vocals={} drums-bass-other={} guitar-piano={}",
                roformer.len(),
                four.len(),
                six.len()
            ),
        });
    }

    // Canonical order: drums, bass, vocals, other, guitar, piano.
    let mut out = Vec::with_capacity(6);
    out.push(four[0].clone()); // drums
    out.push(four[1].clone()); // bass
    out.push(roformer[0].clone()); // vocals (roformer)
    out.push(four[2].clone()); // other
    out.push(six[0].clone()); // guitar
    out.push(six[1].clone()); // piano
    Ok(out)
}

/// Tempdir helper that respects `$TMPDIR` and cleans up on Drop.
fn tempdir() -> std::io::Result<TempDir> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = base.join(format!("openrig-mlx-{pid}-{nanos}"));
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
