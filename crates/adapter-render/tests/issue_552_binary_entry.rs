//! Verifies the `openrig-render` binary's `main` wires argv → `render()`.
//!
//! The other suites cover `render()` directly (library entry). Without
//! this binary-level check, `main.rs` could ship as a stub and the
//! library tests still pass — exactly what happened on the first PR
//! attempt for #552.

use adapter_render::wav::{read_wav, write_wav_stereo, BitDepth};
use std::path::{Path, PathBuf};
use std::process::Command;

fn workdir(test: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("openrig-render-bin-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_minimal_chain(dir: &Path) -> PathBuf {
    let yaml = r#"id: render-bin-test
name: render bin test
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

fn write_mono_sine_wav(path: &Path, sample_rate_hz: u32, duration_s: f32) {
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
fn binary_renders_input_wav_to_output_wav() {
    let dir = workdir("happy_path");
    let chain = write_minimal_chain(&dir);
    let input = dir.join("input.wav");
    let output = dir.join("output.wav");
    write_mono_sine_wav(&input, 48_000, 0.05); // 2400 frames

    let bin = env!("CARGO_BIN_EXE_openrig-render");
    let status = Command::new(bin)
        .args([
            "--chain",
            chain.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--tail-ms",
            "0",
        ])
        .status()
        .expect("failed to spawn openrig-render binary");

    assert!(status.success(), "binary must exit 0 on a valid invocation");
    assert!(
        output.exists(),
        "output.wav must exist after a successful render"
    );

    let data = read_wav(&output).expect("output.wav must be a valid WAV");
    assert_eq!(data.sample_rate_hz, 48_000);
    assert_eq!(data.channels, 2);
    let frames = data.samples.len() / data.channels as usize;
    assert_eq!(frames, 2400, "frame count should match input + 0 tail");
}

#[test]
fn binary_exits_nonzero_when_required_flag_missing() {
    let bin = env!("CARGO_BIN_EXE_openrig-render");
    let status = Command::new(bin)
        .args(["--chain", "/tmp/does_not_matter.yaml"])
        .status()
        .expect("failed to spawn openrig-render binary");

    assert!(
        !status.success(),
        "binary must exit non-zero when --input/--output are missing"
    );
}
