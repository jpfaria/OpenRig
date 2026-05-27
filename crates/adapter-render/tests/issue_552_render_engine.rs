//! Audio-fidelity tests for the offline render driver — real DSP runs.

use adapter_render::cli::RenderArgs;
use adapter_render::render;
use adapter_render::wav::{read_wav, write_wav_stereo, BitDepth};
use std::path::PathBuf;

fn workdir(test: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "openrig-render-engine-{}-{test}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_chain_volume_pct(dir: &std::path::Path, volume_pct: f32) -> PathBuf {
    let yaml = format!(
        r#"id: vol-{volume_pct}
name: volume {volume_pct}
blocks:
- type: gain
  model: volume
  enabled: true
  params:
    volume: {volume_pct:.1}
    mute: false
"#
    );
    let path = dir.join("chain.yaml");
    std::fs::write(&path, yaml).unwrap();
    path
}

fn write_dc_wav(path: &std::path::Path, sample_rate: u32, frames: usize, value: f32) {
    let buf: Vec<[f32; 2]> = (0..frames).map(|_| [value, value]).collect();
    write_wav_stereo(path, &buf, sample_rate, BitDepth::Bits32Float).unwrap();
}

fn mean_abs(samples: &[f32]) -> f32 {
    let sum: f32 = samples.iter().map(|s| s.abs()).sum();
    sum / samples.len().max(1) as f32
}

fn base_args(chain: PathBuf, input: PathBuf, output: PathBuf) -> RenderArgs {
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
        bit_depth: 32,
        tail_ms: 0,
    }
}

#[test]
fn volume_block_at_100_percent_is_audible_passthrough() {
    let dir = workdir("vol_100");
    let chain = write_chain_volume_pct(&dir, 100.0);
    let input = dir.join("input.wav");
    let output = dir.join("output.wav");
    write_dc_wav(&input, 48_000, 4_800, 0.5);

    render(&base_args(chain, input, output.clone())).expect("render succeeds");

    let data = read_wav(&output).unwrap();
    let l_samples: Vec<f32> = data.samples.chunks_exact(2).map(|c| c[0]).collect();
    let amp = mean_abs(&l_samples[1_000..4_000]);
    assert!(
        (amp - 0.5).abs() < 0.05,
        "vol=100% should pass DC=0.5 through: got mean|s|={amp:.4}"
    );
}

#[test]
fn volume_block_at_50_percent_halves_amplitude() {
    let dir = workdir("vol_50");
    let chain = write_chain_volume_pct(&dir, 50.0);
    let input = dir.join("input.wav");
    let output = dir.join("output.wav");
    write_dc_wav(&input, 48_000, 4_800, 0.5);

    render(&base_args(chain, input, output.clone())).expect("render succeeds");

    let data = read_wav(&output).unwrap();
    let l_samples: Vec<f32> = data.samples.chunks_exact(2).map(|c| c[0]).collect();
    let amp = mean_abs(&l_samples[1_000..4_000]);
    assert!(
        (amp - 0.25).abs() < 0.05,
        "vol=50% should halve DC=0.5 → 0.25: got mean|s|={amp:.4}"
    );
}

#[test]
fn render_is_deterministic_byte_for_byte() {
    let dir = workdir("determinism");
    let chain = write_chain_volume_pct(&dir, 100.0);
    let input = dir.join("input.wav");
    let out_a = dir.join("a.wav");
    let out_b = dir.join("b.wav");
    write_dc_wav(&input, 48_000, 2_400, 0.3);

    let mut args = base_args(chain, input, out_a.clone());
    args.tail_ms = 200;
    render(&args).expect("first render");
    args.output = out_b.clone();
    render(&args).expect("second render");

    let bytes_a = std::fs::read(&out_a).unwrap();
    let bytes_b = std::fs::read(&out_b).unwrap();
    assert_eq!(
        bytes_a, bytes_b,
        "same chain + same input → byte-identical output"
    );
}

#[test]
fn render_slice_trims_input() {
    let dir = workdir("slice");
    let chain = write_chain_volume_pct(&dir, 100.0);
    let input = dir.join("input.wav");
    let output = dir.join("output.wav");
    // 0.5 s of DC at 48 kHz = 24000 frames.
    write_dc_wav(&input, 48_000, 24_000, 0.5);

    let mut args = base_args(chain, input, output.clone());
    args.start_s = Some(0.1); // skip 4800 frames
    args.end_s = Some(0.3); // keep up to 14400 frames → 9600 frames kept
    render(&args).expect("render succeeds");

    let data = read_wav(&output).unwrap();
    let actual_frames = data.samples.len() / data.channels as usize;
    let expected = 9_600;
    assert!(
        actual_frames.abs_diff(expected) <= 1,
        "expected ~{expected} frames after slice, got {actual_frames}"
    );
}
