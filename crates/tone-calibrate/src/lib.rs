//! Offline calibration glue (#809, Piece 1 — I/O shell).
//!
//! Walks a corpus of genre-labeled isolated-guitar stems, measures each with
//! [`feature_dsp::tone_descriptors::analyze`], and hands the samples to the pure
//! aggregation core [`feature_dsp::tone_profiles::calibrate`]. All file / WAV /
//! YAML I/O lives here so the maths stays pure and unit-testable.
//!
//! Stem layout mirrors `~/.openrig/evaluations/<song>/refs/{lead,rhythm}.wav`.
//! The genre label is *not* on disk — it comes from a `song -> genre` manifest.

use anyhow::{Context, Result};
use feature_dsp::tone_descriptors::analyze;
use feature_dsp::tone_profiles::{calibrate, Confidence, GenreProfile};
use std::collections::BTreeMap;
use std::path::Path;

/// The two isolated-guitar reference stems each song folder is expected to hold.
const REF_STEMS: [&str; 2] = ["lead.wav", "rhythm.wav"];

/// A `song -> genre` manifest (parsed from YAML: a flat map of folder name to
/// genre label).
pub type Manifest = BTreeMap<String, String>;

/// Measure every labeled stem under `evaluations_root` and calibrate per-genre
/// limits. Missing stems are skipped with a warning to stderr, not an error —
/// a partial corpus still calibrates.
pub fn calibrate_corpus(
    evaluations_root: &Path,
    manifest: &Manifest,
    percentile: f32,
) -> Result<Vec<GenreProfile>> {
    let mut samples: Vec<(String, feature_dsp::tone_descriptors::ToneDescriptors)> = Vec::new();
    for (song, genre) in manifest {
        for stem in REF_STEMS {
            let path = evaluations_root.join(song).join("refs").join(stem);
            if !path.exists() {
                eprintln!("skip: no stem at {}", path.display());
                continue;
            }
            let (frames, sample_rate) = read_wav_stereo(&path)
                .with_context(|| format!("reading stem {}", path.display()))?;
            samples.push((genre.clone(), analyze(&frames, sample_rate)));
        }
    }
    Ok(calibrate(&samples, percentile))
}

/// Read a WAV file as stereo `f32` frames plus its sample rate. Mono is
/// broadcast to both channels; >2 channels keep the first two.
fn read_wav_stereo(path: &Path) -> Result<(Vec<[f32; 2]>, f32)> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let interleaved = read_interleaved(reader, spec)?;

    let mut frames = Vec::with_capacity(interleaved.len() / channels);
    for frame in interleaved.chunks(channels) {
        let l = frame[0];
        let r = if channels >= 2 { frame[1] } else { l };
        frames.push([l, r]);
    }
    Ok((frames, spec.sample_rate as f32))
}

/// Interleaved `f32` samples in `[-1.0, 1.0]`, normalizing any integer PCM
/// depth or float encoding on disk.
fn read_interleaved(
    reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
    spec: hound::WavSpec,
) -> Result<Vec<f32>> {
    let mut reader = reader;
    match spec.sample_format {
        hound::SampleFormat::Float => Ok(reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()?),
        hound::SampleFormat::Int => {
            let scale = match spec.bits_per_sample {
                8 => i8::MAX as f32,
                16 => i16::MAX as f32,
                24 => 8_388_607.0_f32,
                32 => i32::MAX as f32,
                other => anyhow::bail!("unsupported PCM bit depth: {other}"),
            };
            Ok(reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<std::result::Result<Vec<_>, _>>()?)
        }
    }
}

/// Serialize the calibrated table to the versioned `profiles.yaml` form:
/// one entry per genre with its limits plus the evidence (`n`, `confidence`).
pub fn to_yaml(profiles: &[GenreProfile]) -> Result<String> {
    let out: BTreeMap<&str, ProfileEntry> = profiles
        .iter()
        .map(|p| {
            (
                p.genre.as_str(),
                ProfileEntry {
                    mud: p.mud_limit,
                    fizz: p.fizz_limit,
                    clip: p.clip_limit,
                    n: p.n,
                    confidence: match p.confidence {
                        Confidence::Trusted => "trusted",
                        Confidence::Provisional => "provisional",
                    },
                },
            )
        })
        .collect();
    Ok(serde_yaml::to_string(&out)?)
}

/// Serializable shape of one genre row in `profiles.yaml`.
#[derive(serde::Serialize)]
struct ProfileEntry {
    mud: f32,
    fizz: f32,
    clip: f32,
    n: usize,
    confidence: &'static str,
}
