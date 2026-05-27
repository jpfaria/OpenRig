//! Regression test for issue #574: amp block contributions being silently lost.
//! Two different preset YAMLs produced byte-identical output despite different amp models/params.

use adapter_render::cli::RenderArgs;
use adapter_render::render;
use adapter_render::wav::{write_wav_stereo, BitDepth};
use std::path::PathBuf;

fn workdir(test: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "openrig-issue-574-{}-{test}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_dc_wav(path: &std::path::Path, sample_rate: u32, frames: usize, value: f32) {
    let buf: Vec<[f32; 2]> = (0..frames).map(|_| [value, value]).collect();
    write_wav_stereo(path, &buf, sample_rate, BitDepth::Bits32Float).unwrap();
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

/// Test that identical chains produce identical output (baseline).
#[test]
fn identical_chains_produce_identical_output() {
    let dir = workdir("identical");
    let chain = dir.join("chain.yaml");
    std::fs::write(
        &chain,
        r#"id: gain-test-100
name: gain test 100
blocks:
- type: gain
  model: volume
  enabled: true
  params:
    volume: 100.0
    mute: false
"#,
    )
    .unwrap();

    let input = dir.join("input.wav");
    let out_a = dir.join("a.wav");
    let out_b = dir.join("b.wav");
    write_dc_wav(&input, 48_000, 2_400, 0.3);

    render(&base_args(chain.clone(), input.clone(), out_a.clone())).expect("first render");
    render(&base_args(chain, input, out_b.clone())).expect("second render");

    let bytes_a = std::fs::read(&out_a).unwrap();
    let bytes_b = std::fs::read(&out_b).unwrap();
    assert_eq!(
        bytes_a, bytes_b,
        "identical chains should produce byte-identical output"
    );
}

/// Test that different gain blocks produce different output (BUG #574).
/// This test reproduces the core issue: two chains with different parameters
/// render to identical bytes, which should never happen.
#[test]
fn different_gain_should_produce_different_output() {
    let dir = workdir("amp_models");
    let input = dir.join("input.wav");
    write_dc_wav(&input, 48_000, 2_400, 0.3);

    // Chain A: simple volume passthrough at 100%
    let chain_a = dir.join("chain_a.yaml");
    std::fs::write(
        &chain_a,
        r#"id: chain-a
name: Chain A - passthrough
blocks:
- type: gain
  model: volume
  enabled: true
  params:
    volume: 100.0
    mute: false
"#,
    )
    .unwrap();

    // Chain B: same block type but different gain (50%)
    let chain_b = dir.join("chain_b.yaml");
    std::fs::write(
        &chain_b,
        r#"id: chain-b
name: Chain B - with gain reduction
blocks:
- type: gain
  model: volume
  enabled: true
  params:
    volume: 50.0
    mute: false
"#,
    )
    .unwrap();

    let out_a = dir.join("out_a.wav");
    let out_b = dir.join("out_b.wav");

    render(&base_args(chain_a, input.clone(), out_a.clone())).expect("render chain A");
    render(&base_args(chain_b, input, out_b.clone())).expect("render chain B");

    let bytes_a = std::fs::read(&out_a).unwrap();
    let bytes_b = std::fs::read(&out_b).unwrap();

    assert_ne!(
        bytes_a, bytes_b,
        "REGRESSION #574: chains with different gain should produce different bytes, but got identical output"
    );
}

/// Test that toggling block enabled flag produces different output.
/// Per issue #574, toggling amp enabled/disabled produced the SAME bytes.
#[test]
fn toggling_block_enabled_should_change_output() {
    let dir = workdir("enabled_toggle");
    let input = dir.join("input.wav");
    write_dc_wav(&input, 48_000, 2_400, 0.3);

    // Chain A: block enabled with 50% gain
    let chain_enabled = dir.join("chain_enabled.yaml");
    std::fs::write(
        &chain_enabled,
        r#"id: chain-enabled
name: Block enabled
blocks:
- type: gain
  model: volume
  enabled: true
  params:
    volume: 50.0
    mute: false
"#,
    )
    .unwrap();

    // Chain B: same block but disabled (should pass through 100%)
    let chain_disabled = dir.join("chain_disabled.yaml");
    std::fs::write(
        &chain_disabled,
        r#"id: chain-disabled
name: Block disabled
blocks:
- type: gain
  model: volume
  enabled: false
  params:
    volume: 50.0
    mute: false
"#,
    )
    .unwrap();

    let out_enabled = dir.join("out_enabled.wav");
    let out_disabled = dir.join("out_disabled.wav");

    render(&base_args(chain_enabled, input.clone(), out_enabled.clone()))
        .expect("render enabled");
    render(&base_args(chain_disabled, input, out_disabled.clone())).expect("render disabled");

    let bytes_enabled = std::fs::read(&out_enabled).unwrap();
    let bytes_disabled = std::fs::read(&out_disabled).unwrap();

    assert_ne!(
        bytes_enabled, bytes_disabled,
        "REGRESSION #574: enabled block vs disabled block should produce different bytes, but got identical output"
    );
}

/// Test with bundled input using real DI to see if it reproduces the issue.
/// This is closer to the real scenario reported in issue #574.
#[test]
fn render_with_bundled_input_different_gain() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().unwrap().parent().unwrap();
    let bundled_input = repo_root.join("assets/audio/input.wav");

    // If assets don't exist, skip this test gracefully
    if !bundled_input.exists() {
        eprintln!("Skipping: bundled input.wav not found at {:?}", bundled_input);
        return;
    }

    let dir = std::env::temp_dir().join(format!("openrig-issue-574-bundled-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Chain A: simple passthrough
    let chain_a = dir.join("chain_a.yaml");
    std::fs::write(
        &chain_a,
        r#"id: chain-a-passthrough
name: Chain A - passthrough
blocks:
- type: gain
  model: volume
  enabled: true
  params:
    volume: 100.0
    mute: false
"#,
    )
    .unwrap();

    // Chain B: with gain reduction
    let chain_b = dir.join("chain_b.yaml");
    std::fs::write(
        &chain_b,
        r#"id: chain-b-reduced
name: Chain B - gain reduced
blocks:
- type: gain
  model: volume
  enabled: true
  params:
    volume: 50.0
    mute: false
"#,
    )
    .unwrap();

    let out_a = dir.join("out_a.wav");
    let out_b = dir.join("out_b.wav");

    render(&base_args(chain_a, bundled_input.clone(), out_a.clone())).expect("render chain A");
    render(&base_args(chain_b, bundled_input, out_b.clone())).expect("render chain B");

    let bytes_a = std::fs::read(&out_a).unwrap();
    let bytes_b = std::fs::read(&out_b).unwrap();

    assert_ne!(
        bytes_a, bytes_b,
        "Renders with different gain should produce different bytes. If identical, this indicates issue #574."
    );
}
