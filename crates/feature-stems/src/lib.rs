//! Stem separation pipeline for the tracks catalog (issue #553).
//!
//! This crate hosts the off-RT pipeline that turns an audio file into
//! per-stem WAV files using Demucs v4 (`htdemucs`). At this stage the
//! `decode` module exposes a single entry point — additional modules
//! (`resample`, `inference`, `writer`, `peaks`, `model`, `meta`) will
//! be added in following phases.

mod decode;
mod inference;
mod model;
mod pipeline;
mod resample;

pub use decode::decode_audio;
pub use inference::{separate_stems, STEM_COUNT};
pub use model::{ensure_model_with, ModelDownloader, UreqDownloader};
pub use pipeline::{separate_track, SeparateRequest};
pub use resample::resample_to;

use std::path::PathBuf;

/// Audio decoded from a source file, interleaved stereo (or duplicated
/// mono), in normalized `f32` samples at the source sample rate.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    /// Interleaved stereo samples: `[L0, R0, L1, R1, ...]`.
    pub samples: Vec<f32>,
    /// Sample rate of the decoded audio, in Hz.
    pub sample_rate: u32,
    /// Channel count of the *source* file (1 for mono, 2+ for multichannel).
    /// `samples` is always stereo — mono sources are broadcast to both
    /// channels per the OpenRig stereo invariant.
    pub source_channels: u16,
}

/// Errors produced by the stem-separation pipeline.
#[derive(Debug, thiserror::Error)]
pub enum StemError {
    /// Source file could not be opened.
    #[error("cannot open source file `{path}`: {source}", path = path.display())]
    OpenSource {
        /// Source path that failed to open.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Source file format is unsupported or the container is malformed.
    #[error("unsupported or malformed audio in `{path}`: {reason}", path = path.display())]
    UnsupportedFormat {
        /// Source path that produced the failure.
        path: PathBuf,
        /// Human-readable reason.
        reason: String,
    },

    /// Decoder failed while reading frames from the source.
    #[error("decode failure in `{path}`: {reason}", path = path.display())]
    Decode {
        /// Source path that produced the failure.
        path: PathBuf,
        /// Human-readable reason.
        reason: String,
    },

    /// Resampler failed to convert sample rate.
    #[error("resample failure: {reason}")]
    Resample {
        /// Human-readable reason.
        reason: String,
    },

    /// Model download / cache verification failed.
    #[error("model download failure: {reason}")]
    ModelDownload {
        /// Human-readable reason.
        reason: String,
    },
}
