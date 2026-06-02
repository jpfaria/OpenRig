//! Task 4 — off-thread DI source preload (decode + resample + crossfade).
//! RED-FIRST test: asserts `load_di_loop` + `DiLoopSource` exist in `application`
//! and that a mono 24 kHz file is decoded, resampled to 48 kHz, and produces a
//! non-trivial `DiLoop`.

use application::di_loader::{load_di_loop, DiLoopSource};
use block_core::AudioChannelLayout;

/// Write a minimal valid mono WAV (PCM f32) with `num_frames` frames at `sr`.
fn write_mono_wav(path: &std::path::Path, sr: u32, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("WavWriter::create");
    for &s in samples {
        w.write_sample(s).expect("write_sample");
    }
    w.finalize().expect("finalize");
}

#[test]
fn load_di_loop_file_resamples_and_returns_correct_layout() {
    // 4-frame mono ramp at 24 kHz.
    let src_samples: Vec<f32> = (0..4).map(|i| i as f32 / 3.0).collect();

    let dir = tempfile::tempdir().expect("tempdir");
    let wav_path = dir.path().join("test_di.wav");
    write_mono_wav(&wav_path, 24_000, &src_samples);

    let result = load_di_loop(&DiLoopSource::File(wav_path), 48_000);
    let di = result.expect("load_di_loop must succeed for a valid file");

    // 4 frames @ 24 kHz resampled to 48 kHz ≈ 8 frames (≥ 7 with rounding).
    assert!(
        di.len() >= 7,
        "expected len >= 7 after 24k→48k resample, got {}",
        di.len()
    );

    assert_eq!(
        di.layout(),
        AudioChannelLayout::Mono,
        "mono source must produce Mono DiLoop"
    );
}

#[test]
fn load_di_loop_missing_file_returns_err() {
    let result = load_di_loop(
        &DiLoopSource::File(std::path::PathBuf::from("/nonexistent/path/di.wav")),
        48_000,
    );
    assert!(
        result.is_err(),
        "missing file must return Err, not Ok"
    );
}
