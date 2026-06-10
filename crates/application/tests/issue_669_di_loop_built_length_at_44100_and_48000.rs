//! Guard (#669/#670): a DI loop's built length must track the engine sample
//! rate at BOTH 44.1 kHz and 48 kHz. A loop is played 1:1 (one buffer frame per
//! device frame), so if it is built at the wrong rate its DURATION/pitch is
//! wrong — the "slow motion at 44.1 kHz" bug. The dispatcher-level resample is
//! the single place the device rate enters the loop, so we pin the resulting
//! frame count for each rate from one fixed-rate source.
//!
//! This is the coverage that was missing: earlier tests asserted a relative
//! length (44.1k < 48k) but never the exact rate-scaled count at each rate, so
//! a regression that built every loop at 48 kHz could still pass.

use std::path::Path;

use application::di_loader::{load_di_loop, DiLoopSource, DI_LOOP_XFADE_FRAMES};

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

/// Expected built length: resample `src_frames` from 48 kHz to `engine_sr`
/// (linear, round), then drop `DI_LOOP_XFADE_FRAMES` for the seam crossfade.
fn expected_len(src_frames: usize, engine_sr: u32) -> usize {
    let resampled = ((src_frames as f64) * (engine_sr as f64) / 48_000.0).round() as usize;
    resampled - DI_LOOP_XFADE_FRAMES
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

    let load_len = |engine_sr: u32| {
        load_di_loop(&DiLoopSource::File(wav.clone()), engine_sr)
            .expect("load_di_loop must succeed")
            .len()
    };

    let len_48 = load_len(48_000);
    let len_44 = load_len(44_100);

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
