//! Guard (#669/#670/#749): a DI loop's built length must track the OUTPUT
//! sample rate at BOTH 44.1 kHz and 48 kHz. A loop is played 1:1 (one buffer
//! frame per device frame), so if it is built at the wrong rate its
//! DURATION/pitch is wrong — the "slow motion at 44.1 kHz" bug.
//!
//! #749 moved the resample out of the loader (which used a single `engine_sr`
//! and stretched the mismatched output on a multi-rate rig) and into the arm
//! path, per output-stream rate: `load_di_loop` now returns the un-resampled
//! `DiPcm` source, and `DiPcm::to_loop_at(rate)` builds the rate-correct loop.
//! We pin the resulting frame count for each rate from one fixed-rate source.

use std::path::Path;

use application::di_loader::{load_di_loop, DiLoopSource};

fn write_mono_wav(path: &Path, sr: u32, samples: &[f32]) {
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

/// Expected built length: resample `src_frames` from 48 kHz to `target_sr`
/// (linear, round), then drop the seam crossfade (~10 ms = `target_sr/100`,
/// matching `DiPcm::to_loop_at`).
fn expected_len(src_frames: usize, target_sr: u32) -> usize {
    let resampled = ((src_frames as f64) * (target_sr as f64) / 48_000.0).round() as usize;
    resampled - (target_sr / 100) as usize
}

#[test]
fn di_loop_built_length_tracks_engine_rate_at_44100_and_48000() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("src_48k.wav");
    // A 48 kHz source long enough to spare the crossfade at both rates.
    let src_frames = 4800usize;
    let samples: Vec<f32> = (0..src_frames)
        .map(|i| ((i % 64) as f32 / 64.0) - 0.5)
        .collect();
    write_mono_wav(&wav, 48_000, &samples);

    // #749: decode once (un-resampled), then build the loop at each output rate.
    let pcm = load_di_loop(&DiLoopSource::File(wav.clone())).expect("load_di_loop must succeed");
    let len_48 = pcm.to_loop_at(48_000).len();
    let len_44 = pcm.to_loop_at(44_100).len();

    // 48 kHz: source already at the engine rate → identity resample.
    assert_eq!(
        len_48,
        expected_len(src_frames, 48_000),
        "48 kHz: loop must keep its source length (identity), got {len_48}"
    );
    // 44.1 kHz: resampled DOWN — fewer frames, in exact proportion.
    assert_eq!(
        len_44,
        expected_len(src_frames, 44_100),
        "44.1 kHz: loop must be resampled to the device rate (≈0.919×), got {len_44}"
    );
    // And the two rates must differ — a regression that builds every loop at a
    // single hardcoded rate (the slow-motion bug) collapses this.
    assert!(
        len_44 < len_48,
        "44.1 kHz loop ({len_44}) must be shorter than 48 kHz ({len_48})"
    );
}
