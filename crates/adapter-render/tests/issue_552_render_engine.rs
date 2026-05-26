//! Red-first tests for real engine processing in the offline renderer
//! (issue #552, phase P4b).
//!
//! These tests assert audio fidelity, not just frame counts — the chain's
//! DSP MUST actually run on the samples. The shell from phase P4 only
//! padded the input with silence; it satisfies the count tests but NOT
//! these.

use adapter_render::cli::RenderArgs;
use adapter_render::render;
use adapter_render::wav::{read_wav, write_wav_stereo, BitDepth};
use std::path::PathBuf;

fn workdir(test: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("openrig-render-p4b-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_project_volume_50(dir: &std::path::Path) -> PathBuf {
    // One chain, a single gain/volume block at 50% (= 0.5 linear).
    // The chain has no input/output device blocks — the offline driver
    // supplies the bus directly.
    let yaml = r#"chains:
  - description: render-test-volume-50
    instrument: electric_guitar
    enabled: true
    blocks:
      - type: gain
        model: volume
        enabled: true
        params:
          volume: 50.0
          mute: false
"#;
    let path = dir.join("project.openrig");
    std::fs::write(&path, yaml).unwrap();
    path
}

fn write_project_passthrough(dir: &std::path::Path) -> PathBuf {
    // Volume block at 100% (= 1.0 linear) — analytical passthrough.
    let yaml = r#"chains:
  - description: render-test-passthrough
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

fn write_constant_dc_wav(path: &std::path::Path, sample_rate: u32, frames: usize, value: f32) {
    let buf: Vec<[f32; 2]> = (0..frames).map(|_| [value, value]).collect();
    write_wav_stereo(path, &buf, sample_rate, BitDepth::Bits32Float).unwrap();
}

/// Mean absolute amplitude across a contiguous range of output frames.
fn mean_abs(samples: &[f32]) -> f32 {
    let sum: f32 = samples.iter().map(|s| s.abs()).sum();
    sum / samples.len().max(1) as f32
}

#[test]
fn volume_block_at_100_percent_is_audible_passthrough() {
    let dir = workdir("vol_100_passthrough");
    let project = write_project_passthrough(&dir);
    let input = dir.join("input.wav");
    let output = dir.join("output.wav");

    // 0.1 s of DC at 0.5 (well above noise floor, below clipping).
    let frames = 4_800;
    write_constant_dc_wav(&input, 48_000, frames, 0.5);

    let args = RenderArgs {
        project,
        input,
        output: output.clone(),
        chain: None,
        sample_rate_hz: 48_000,
        block_size: 256,
        bit_depth: 32,
        tail_ms: 0,
    };

    render(&args).expect("render succeeds");

    let data = read_wav(&output).unwrap();
    // The first frames region (post any startup transient) should be at
    // ~0.5 amplitude — within 5% of the input.
    let l_samples: Vec<f32> = data.samples.chunks_exact(2).map(|c| c[0]).collect();
    let stable_window = &l_samples[1_000..4_000];
    let amp = mean_abs(stable_window);
    assert!(
        (amp - 0.5).abs() < 0.05,
        "vol=100% should pass DC=0.5 through: got mean|s|={amp:.4}"
    );
}

#[test]
fn volume_block_at_50_percent_halves_amplitude() {
    let dir = workdir("vol_50_halves");
    let project = write_project_volume_50(&dir);
    let input = dir.join("input.wav");
    let output = dir.join("output.wav");

    let frames = 4_800;
    write_constant_dc_wav(&input, 48_000, frames, 0.5);

    let args = RenderArgs {
        project,
        input,
        output: output.clone(),
        chain: None,
        sample_rate_hz: 48_000,
        block_size: 256,
        bit_depth: 32,
        tail_ms: 0,
    };

    render(&args).expect("render succeeds");

    let data = read_wav(&output).unwrap();
    let l_samples: Vec<f32> = data.samples.chunks_exact(2).map(|c| c[0]).collect();
    let stable_window = &l_samples[1_000..4_000];
    let amp = mean_abs(stable_window);
    let expected = 0.25; // 0.5 input × 0.5 volume = 0.25
    assert!(
        (amp - expected).abs() < 0.05,
        "vol=50% should halve DC=0.5 → 0.25: got mean|s|={amp:.4}"
    );
}

#[test]
fn render_is_deterministic_byte_for_byte() {
    let dir = workdir("determinism");
    let project = write_project_passthrough(&dir);
    let input = dir.join("input.wav");
    let out_a = dir.join("a.wav");
    let out_b = dir.join("b.wav");

    write_constant_dc_wav(&input, 48_000, 2_400, 0.3);

    let base = RenderArgs {
        project,
        input,
        output: out_a.clone(),
        chain: None,
        sample_rate_hz: 48_000,
        block_size: 256,
        bit_depth: 32,
        tail_ms: 200,
    };

    render(&base).expect("first render");
    let mut second = base.clone();
    second.output = out_b.clone();
    render(&second).expect("second render");

    let bytes_a = std::fs::read(&out_a).unwrap();
    let bytes_b = std::fs::read(&out_b).unwrap();
    assert_eq!(
        bytes_a, bytes_b,
        "same project + same input → byte-identical output"
    );
}
