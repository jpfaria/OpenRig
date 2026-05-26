//! End-to-end orchestrator: source file → tracks catalog entry.
//!
//! Glues together [`decode_audio`](crate::decode_audio),
//! [`resample_to`](crate::resample_to),
//! [`separate_stems`](crate::separate_stems), and the WAV writer to
//! produce a complete `<catalog>/<id>/` directory with `meta.yaml` and
//! per-stem WAVs. Designed to run off-RT on a worker thread.

use std::fs;
use std::path::PathBuf;

use feature_tracks::{StemInfo, StemKind, TrackEntry, TrackId, TrackMeta};
use hound::{SampleFormat, WavSpec, WavWriter};

use crate::{decode_audio, inference::separate_stems, resample_to, tags::extract_tags, StemError};

/// Pick the real htdemucs path when the `real-htdemucs` feature is on
/// and the ONNX model is present, otherwise fall back to the stub.
fn run_separation(samples: &[f32], sample_rate: u32) -> Result<Vec<Vec<f32>>, StemError> {
    #[cfg(feature = "real-htdemucs")]
    {
        let model_path = htdemucs_model_path();
        if model_path.exists() {
            return crate::inference_ort::separate_stems_with_ort(
                samples,
                sample_rate,
                &model_path,
            );
        }
    }
    separate_stems(samples, sample_rate)
}

/// Canonical disk location for the htdemucs ONNX model.
#[cfg(feature = "real-htdemucs")]
fn htdemucs_model_path() -> PathBuf {
    dirs::data_dir()
        .map(|d| {
            d.join("OpenRig")
                .join("models")
                .join("htdemucs")
                .join("htdemucs.onnx")
        })
        .unwrap_or_else(|| PathBuf::from("models/htdemucs/htdemucs.onnx"))
}

/// htdemucs operates at this rate.
const MODEL_SAMPLE_RATE: u32 = 44_100;

/// Description of a separation job submitted to [`separate_track`].
#[derive(Debug, Clone)]
pub struct SeparateRequest {
    /// Path to the source audio file (WAV/MP3/FLAC/OGG/M4A).
    pub source_path: PathBuf,
    /// Root of the tracks catalog (`<presets-path>/tracks/`).
    pub catalog_dir: PathBuf,
    /// Stable id for the new track — also the directory name.
    pub track_id: String,
    /// Display title (auto-fill ID3/Vorbis tag in callers).
    pub title: String,
    /// Model identifier recorded in `meta.yaml` (e.g. `htdemucs`,
    /// `stub`).
    pub model: String,
    /// ISO 8601 timestamp recorded in `meta.yaml`. Callers inject so
    /// tests can pin to a fixed value.
    pub generated_at: String,
}

/// Run the full pipeline against `request` and return the persisted
/// [`TrackEntry`].
///
/// # Errors
///
/// Propagates every [`StemError`] from decode, resample, separation,
/// and WAV write stages. Partial output (e.g. the track dir + a few
/// stems) is left on disk so the caller can decide to clean up.
pub fn separate_track(request: &SeparateRequest) -> Result<TrackEntry, StemError> {
    let decoded = decode_audio(&request.source_path)?;
    let source_sr = decoded.sample_rate;
    let work = resample_to(&decoded.samples, source_sr, MODEL_SAMPLE_RATE)?;

    let stems = run_separation(&work, MODEL_SAMPLE_RATE)?;

    let track_dir = request.catalog_dir.join(&request.track_id);
    fs::create_dir_all(&track_dir).map_err(|err| StemError::OpenSource {
        path: track_dir.clone(),
        source: err,
    })?;

    let kinds = [
        StemKind::Drums,
        StemKind::Bass,
        StemKind::Vocals,
        StemKind::Other,
    ];
    let mut stem_meta = Vec::with_capacity(kinds.len());
    for (kind, stem_samples) in kinds.into_iter().zip(stems.iter()) {
        let resampled_back = resample_to(stem_samples, MODEL_SAMPLE_RATE, source_sr)?;
        let filename = kind.default_filename();
        let path = track_dir.join(filename);
        write_stereo_wav(&path, &resampled_back, source_sr)?;
        stem_meta.push(StemInfo {
            kind,
            filename: filename.to_string(),
        });
    }

    let frames = (decoded.samples.len() / 2) as f64;
    let duration_secs = frames / source_sr as f64;
    let extracted = extract_tags(&request.source_path).unwrap_or_default();

    let meta = TrackMeta {
        id: TrackId::new(&request.track_id),
        // Caller-supplied title wins; fall back to the ID3/Vorbis
        // title when the caller passes an empty string.
        title: if request.title.is_empty() {
            extracted.title.unwrap_or_default()
        } else {
            request.title.clone()
        },
        artist: extracted.artist,
        album: extracted.album,
        year: extracted.year,
        genre: extracted.genre,
        bpm: None,
        key: None,
        duration_secs,
        source_sample_rate: source_sr,
        stems: stem_meta,
        model: request.model.clone(),
        generated_at: request.generated_at.clone(),
    };
    let entry = TrackEntry {
        meta,
        dir: track_dir,
    };
    entry.save().map_err(|err| StemError::OpenSource {
        path: entry.dir.clone(),
        source: std::io::Error::other(err.to_string()),
    })?;
    Ok(entry)
}

fn write_stereo_wav(path: &PathBuf, samples: &[f32], sample_rate: u32) -> Result<(), StemError> {
    let spec = WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec).map_err(|err| StemError::OpenSource {
        path: path.clone(),
        source: std::io::Error::other(err.to_string()),
    })?;
    for &sample in samples {
        writer
            .write_sample(sample)
            .map_err(|err| StemError::OpenSource {
                path: path.clone(),
                source: std::io::Error::other(err.to_string()),
            })?;
    }
    writer.finalize().map_err(|err| StemError::OpenSource {
        path: path.clone(),
        source: std::io::Error::other(err.to_string()),
    })?;
    Ok(())
}
