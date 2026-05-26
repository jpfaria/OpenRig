//! End-to-end test for the offline render driver — orchestration shell
//! (chain load, WAV I/O, tail, atomic write). Audio fidelity is verified
//! by `issue_552_render_engine.rs`.

use adapter_render::cli::RenderArgs;
use adapter_render::wav::{read_wav, write_wav_stereo, BitDepth};
use adapter_render::{render, RenderError};
use std::path::PathBuf;

fn workdir(test: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "openrig-render-shell-{}-{test}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_minimal_chain(dir: &std::path::Path) -> PathBuf {
    // Smallest valid preset YAML the loader accepts: a flat block list
    // (presets/clean.yaml shape). No input/output blocks — the offline
    // render driver supplies the bus directly.
    let yaml = r#"id: render-shell-test
name: render shell test
blocks:
- type: gain
  model: volume
  enabled: true
  params:
    volume: 100.0
    mute: false
"#;
    let path = dir.join("chain.yaml");
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

fn base_args(chain: PathBuf, input: PathBuf, output: PathBuf, tail_ms: u32) -> RenderArgs {
    RenderArgs {
        chain,
        input,
        output,
        start_s: None,
        end_s: None,
        duration_s: None,
        input_device: None,
        sample_rate_hz: 48_000,
        block_size: 256,
        bit_depth: 24,
        tail_ms,
    }
}

#[test]
fn render_emits_output_wav_with_input_plus_tail_frames() {
    let dir = workdir("plus_tail");
    let chain = write_minimal_chain(&dir);
    let input = dir.join("input.wav");
    let output = dir.join("output.wav");

    write_mono_sine_wav(&input, 48_000, 0.1); // 4800 frames

    let args = base_args(chain, input, output.clone(), 500); // 24000 frames tail
    render(&args).expect("render must succeed for a minimal chain");

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
fn render_errors_when_chain_file_missing() {
    let dir = workdir("missing_chain");
    let input = dir.join("input.wav");
    let output = dir.join("output.wav");
    write_mono_sine_wav(&input, 48_000, 0.05);

    let args = base_args(dir.join("does_not_exist.yaml"), input, output, 0);
    let err = render(&args).expect_err("missing chain must error");
    assert!(matches!(err, RenderError::ChainLoad(_)), "got {err:?}");
}

#[test]
fn render_errors_when_input_wav_missing_and_no_duration() {
    let dir = workdir("missing_input_no_capture");
    let chain = write_minimal_chain(&dir);
    let output = dir.join("output.wav");

    let args = base_args(chain, dir.join("does_not_exist.wav"), output, 0);
    let err = render(&args).expect_err("missing input + no --duration must error");
    assert!(matches!(err, RenderError::InvalidArgs(_)), "got {err:?}");
}

#[test]
fn render_does_not_leave_partial_output_on_failure() {
    let dir = workdir("atomic_no_partial");
    let chain = write_minimal_chain(&dir);
    let output = dir.join("output.wav");

    let args = base_args(chain, dir.join("does_not_exist.wav"), output.clone(), 0);
    let _ = render(&args); // expected to fail
    assert!(
        !output.exists(),
        "output.wav must not exist after a failed render"
    );
}
