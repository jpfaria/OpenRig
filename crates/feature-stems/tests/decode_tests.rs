//! RED-first tests for the audio decode entry point of `feature-stems`.
//!
//! These tests exercise `feature_stems::decode_audio` against on-disk WAV
//! fixtures generated at runtime to keep the repository free of binary
//! blobs.

use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};

const SAMPLE_RATE: u32 = 48_000;
const DURATION_SECS: u32 = 1;
const FREQ_HZ: f32 = 440.0;

fn write_sine_wav(path: &Path, channels: u16) {
    let spec = WavSpec {
        channels,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec).expect("create wav writer");
    let total_frames = (SAMPLE_RATE * DURATION_SECS) as usize;
    for frame in 0..total_frames {
        let t = frame as f32 / SAMPLE_RATE as f32;
        let sample = (t * FREQ_HZ * std::f32::consts::TAU).sin() * 0.5;
        for _ in 0..channels {
            writer.write_sample(sample).expect("write sample");
        }
    }
    writer.finalize().expect("finalize wav");
}

#[test]
fn decode_mono_wav_broadcasts_to_stereo_and_preserves_sample_rate() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sine_mono.wav");
    write_sine_wav(&path, 1);

    let decoded = feature_stems::decode_audio(&path).expect("decode mono wav");

    assert_eq!(decoded.sample_rate, SAMPLE_RATE);
    assert_eq!(decoded.source_channels, 1);
    // 1 second mono broadcast to stereo = SAMPLE_RATE frames * 2 samples.
    assert_eq!(decoded.samples.len(), (SAMPLE_RATE as usize) * 2);
    // Left/right must be identical on every frame (mono broadcast invariant).
    for frame in decoded.samples.chunks_exact(2) {
        assert!(
            (frame[0] - frame[1]).abs() < f32::EPSILON,
            "mono broadcast must keep L == R, got L={} R={}",
            frame[0],
            frame[1]
        );
    }
}

#[test]
fn decode_stereo_wav_preserves_channels_and_frame_count() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("sine_stereo.wav");
    write_sine_wav(&path, 2);

    let decoded = feature_stems::decode_audio(&path).expect("decode stereo wav");

    assert_eq!(decoded.sample_rate, SAMPLE_RATE);
    assert_eq!(decoded.source_channels, 2);
    assert_eq!(decoded.samples.len(), (SAMPLE_RATE as usize) * 2);
}

#[test]
fn decode_missing_path_returns_open_source_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does_not_exist.wav");

    let err = feature_stems::decode_audio(&missing).expect_err("must fail on missing file");
    assert!(
        matches!(err, feature_stems::StemError::OpenSource { .. }),
        "expected OpenSource error, got {err:?}"
    );
}
