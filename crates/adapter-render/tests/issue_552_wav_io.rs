//! Red-first tests for WAV I/O + mono→stereo broadcast (issue #552).

use adapter_render::wav::{
    broadcast_mono_to_stereo, interleaved_to_stereo_frames, read_wav, write_wav_stereo,
    BitDepth, WavData, WavError,
};
use std::path::PathBuf;

fn tmp_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("openrig-render-tests-{}-{name}", std::process::id()));
    p
}

#[test]
fn read_wav_returns_error_when_path_missing() {
    let path = tmp_path("does_not_exist.wav");
    let err = read_wav(&path).expect_err("missing file must error");
    assert!(matches!(err, WavError::Io(_)), "expected Io error, got {err:?}");
}

#[test]
fn write_and_read_mono_round_trip() {
    let path = tmp_path("mono_round_trip.wav");
    // Build a 0.01 s mono signal at 48 kHz: 480 samples linear ramp.
    let frames: Vec<[f32; 2]> = (0..480)
        .map(|i| {
            let s = (i as f32) / 480.0;
            [s, s] // same value on both channels — will be detected as mono-correlated
        })
        .collect();
    write_wav_stereo(&path, &frames, 48_000, BitDepth::Bits24).unwrap();

    let data: WavData = read_wav(&path).unwrap();
    assert_eq!(data.sample_rate_hz, 48_000);
    assert_eq!(data.channels, 2);
    assert_eq!(data.samples.len(), frames.len() * 2);

    // Cleanup.
    let _ = std::fs::remove_file(&path);
}

#[test]
fn broadcast_mono_to_stereo_duplicates_each_sample() {
    let mono: Vec<f32> = vec![0.0, 0.25, -0.5, 1.0];
    let stereo = broadcast_mono_to_stereo(&mono);
    assert_eq!(stereo.len(), mono.len());
    for (i, frame) in stereo.iter().enumerate() {
        assert_eq!(frame[0], mono[i], "L channel mismatch at frame {i}");
        assert_eq!(frame[1], mono[i], "R channel mismatch at frame {i}");
    }
}

#[test]
fn interleaved_to_stereo_frames_handles_stereo_input() {
    // 3 stereo frames: [L0, R0, L1, R1, L2, R2]
    let interleaved: Vec<f32> = vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3];
    let frames = interleaved_to_stereo_frames(&interleaved, 2);
    assert_eq!(frames, vec![[0.1, -0.1], [0.2, -0.2], [0.3, -0.3]]);
}

#[test]
fn interleaved_to_stereo_frames_broadcasts_mono_input() {
    // Mono input — must be broadcast to stereo.
    let interleaved: Vec<f32> = vec![0.1, 0.2, 0.3];
    let frames = interleaved_to_stereo_frames(&interleaved, 1);
    assert_eq!(frames, vec![[0.1, 0.1], [0.2, 0.2], [0.3, 0.3]]);
}

#[test]
fn write_wav_stereo_supports_16_24_32_bit_depths() {
    for depth in [BitDepth::Bits16, BitDepth::Bits24, BitDepth::Bits32Float] {
        let path = tmp_path(&format!("bitdepth_{depth:?}.wav"));
        let frames = vec![[0.0_f32, 0.0_f32], [0.5, -0.5], [-1.0, 1.0]];
        write_wav_stereo(&path, &frames, 44_100, depth).unwrap();

        let data = read_wav(&path).unwrap();
        assert_eq!(data.sample_rate_hz, 44_100);
        assert_eq!(data.channels, 2);
        assert_eq!(data.samples.len(), frames.len() * 2);
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn write_wav_stereo_is_deterministic_byte_for_byte() {
    let path_a = tmp_path("determinism_a.wav");
    let path_b = tmp_path("determinism_b.wav");
    let frames: Vec<[f32; 2]> = (0..1024)
        .map(|i| {
            let s = ((i as f32) * 0.1).sin();
            [s, -s]
        })
        .collect();

    write_wav_stereo(&path_a, &frames, 48_000, BitDepth::Bits24).unwrap();
    write_wav_stereo(&path_b, &frames, 48_000, BitDepth::Bits24).unwrap();

    let bytes_a = std::fs::read(&path_a).unwrap();
    let bytes_b = std::fs::read(&path_b).unwrap();
    assert_eq!(bytes_a, bytes_b, "same input must produce byte-identical WAVs");

    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
}
