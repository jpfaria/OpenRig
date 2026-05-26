//! Red-first end-to-end test for the offline render driver (issue #552).
//!
//! The first iteration asserts only the orchestration shell: load a tiny
//! project file, read an input WAV, write an output WAV with the expected
//! frame count (input + tail). Audio fidelity is verified by the golden
//! tests in a later phase; this test exists to drive the `render()` entry
//! point into existence.

use adapter_render::cli::RenderArgs;
use adapter_render::wav::{read_wav, write_wav_stereo, BitDepth};
use adapter_render::{render, RenderError};
use std::path::PathBuf;

fn workdir(test: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("openrig-render-p4-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_minimal_project(dir: &std::path::Path) -> PathBuf {
    // Smallest valid project the loader accepts: one chain, one passthrough
    // block. Input/output blocks are intentionally omitted — the offline
    // render driver supplies the bus directly via the input/output WAVs.
    let yaml = r#"chains:
  - description: render-test
    instrument: electric_guitar
    enabled: true
    blocks:
      - type: gain
        model: volume
        enabled: true
        params:
          volume: 100.0
          mute: false
"#;
    let path = dir.join("project.openrig");
    std::fs::write(&path, yaml).unwrap();
    path
}

fn write_mono_sine_wav(path: &std::path::Path, sample_rate_hz: u32, duration_s: f32) {
    let n = (sample_rate_hz as f32 * duration_s) as usize;
    let frames: Vec<[f32; 2]> = (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate_hz as f32;
            let s = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
            [s, s]
        })
        .collect();
    write_wav_stereo(path, &frames, sample_rate_hz, BitDepth::Bits24).unwrap();
}

#[test]
fn render_emits_output_wav_with_input_plus_tail_frames() {
    let dir = workdir("plus_tail");
    let project = write_minimal_project(&dir);
    let input = dir.join("input.wav");
    let output = dir.join("output.wav");

    write_mono_sine_wav(&input, 48_000, 0.1); // 4800 frames

    let args = RenderArgs {
        project,
        input,
        output: output.clone(),
        chain: None,
        sample_rate_hz: 48_000,
        block_size: 256,
        bit_depth: 24,
        tail_ms: 500, // → 24000 frames
    };

    render(&args).expect("render must succeed for a minimal valid project");

    let data = read_wav(&output).expect("output wav must exist");
    assert_eq!(data.sample_rate_hz, 48_000);
    assert_eq!(data.channels, 2);

    let expected_frames = 4800 + (48_000 * 500 / 1000);
    let actual_frames = data.samples.len() / data.channels as usize;
    assert_eq!(
        actual_frames, expected_frames as usize,
        "frame count should be input + tail"
    );
}

#[test]
fn render_errors_when_project_file_missing() {
    let dir = workdir("missing_project");
    let input = dir.join("input.wav");
    let output = dir.join("output.wav");
    write_mono_sine_wav(&input, 48_000, 0.05);

    let args = RenderArgs {
        project: dir.join("does_not_exist.openrig"),
        input,
        output,
        chain: None,
        sample_rate_hz: 48_000,
        block_size: 256,
        bit_depth: 24,
        tail_ms: 0,
    };

    let err = render(&args).expect_err("missing project must error");
    assert!(matches!(err, RenderError::ProjectLoad(_)));
}

#[test]
fn render_errors_when_input_wav_missing() {
    let dir = workdir("missing_input");
    let project = write_minimal_project(&dir);
    let output = dir.join("output.wav");

    let args = RenderArgs {
        project,
        input: dir.join("does_not_exist.wav"),
        output,
        chain: None,
        sample_rate_hz: 48_000,
        block_size: 256,
        bit_depth: 24,
        tail_ms: 0,
    };

    let err = render(&args).expect_err("missing input wav must error");
    assert!(matches!(err, RenderError::InputRead(_)));
}

#[test]
fn render_does_not_leave_partial_output_on_failure() {
    let dir = workdir("atomic_no_partial");
    let project = write_minimal_project(&dir);
    let output = dir.join("output.wav");

    let args = RenderArgs {
        project,
        input: dir.join("does_not_exist.wav"),
        output: output.clone(),
        chain: None,
        sample_rate_hz: 48_000,
        block_size: 256,
        bit_depth: 24,
        tail_ms: 0,
    };

    let _ = render(&args); // expected to fail
    assert!(
        !output.exists(),
        "output.wav must not exist after a failed render"
    );
}
