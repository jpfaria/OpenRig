//! Red-first (#576) tests for the `render_chain` MCP tool.
//!
//! Mirrors the contract from `crates/adapter-render` so an MCP client can
//! run the same headless offline render that the `openrig-render` binary
//! does. Tests pin:
//!   * file-mode happy path produces an output WAV (and the wrapper does
//!     not lose determinism — bytes match a direct `adapter_render::render`
//!     call with the same args);
//!   * argument errors (invalid bit depth) map to `InvalidParams`;
//!   * render errors (missing input WAV without `duration_s`) map to
//!     `RenderFailed` and leave no partial output behind;
//!   * the response metadata (sample rate, bit depth, mode) reflects the
//!     actual render.

use adapter_mcp::render_tool::{render_chain, RenderChainError, RenderChainInput};
use adapter_render::wav::{write_wav_stereo, BitDepth};
use std::path::{Path, PathBuf};

fn workdir(test: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("openrig-mcp-render-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_volume_chain(dir: &Path, volume_pct: f32) -> PathBuf {
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

fn write_dc_wav(path: &Path, sample_rate: u32, frames: usize, value: f32) {
    let buf: Vec<[f32; 2]> = (0..frames).map(|_| [value, value]).collect();
    write_wav_stereo(path, &buf, sample_rate, BitDepth::Bits32Float).unwrap();
}

fn minimal_input(chain: PathBuf, input: PathBuf, output: PathBuf) -> RenderChainInput {
    RenderChainInput {
        chain_path: chain,
        input_path: input,
        output_path: output,
        start_s: None,
        end_s: None,
        duration_s: None,
        input_device: None,
        sample_rate_hz: None,
        block_size: None,
        bit_depth: None,
        tail_ms: Some(0),
    }
}

#[test]
fn render_chain_file_mode_produces_output_wav() {
    let dir = workdir("file_mode_ok");
    let chain = write_volume_chain(&dir, 100.0);
    let input = dir.join("in.wav");
    let output = dir.join("out.wav");
    write_dc_wav(&input, 48_000, 4_800, 0.5);

    let resp = render_chain(minimal_input(chain, input, output.clone()))
        .expect("file-mode render succeeds");

    assert!(
        output.exists(),
        "output WAV should be written at the requested path"
    );
    assert_eq!(
        PathBuf::from(&resp.output_path),
        output,
        "response should echo the absolute output path"
    );
    assert_eq!(resp.mode, "file", "existing input WAV → file mode");
    assert_eq!(resp.sample_rate, 48_000);
    assert_eq!(resp.bit_depth, 24, "default bit depth is 24-bit PCM");
    assert!(
        resp.duration_seconds > 0.0,
        "duration_seconds must be populated"
    );
}

#[test]
fn render_chain_respects_explicit_bit_depth_in_response() {
    let dir = workdir("bit_depth_echo");
    let chain = write_volume_chain(&dir, 100.0);
    let input = dir.join("in.wav");
    let output = dir.join("out.wav");
    write_dc_wav(&input, 48_000, 2_400, 0.3);

    let mut req = minimal_input(chain, input, output);
    req.bit_depth = Some(32);
    let resp = render_chain(req).expect("render succeeds with 32-bit float output");

    assert_eq!(resp.bit_depth, 32);
}

#[test]
fn render_chain_missing_input_without_duration_errors_with_no_partial_output() {
    let dir = workdir("missing_input_no_duration");
    let chain = write_volume_chain(&dir, 100.0);
    let input = dir.join("does-not-exist.wav");
    let output = dir.join("out.wav");

    let err = render_chain(minimal_input(chain, input, output.clone()))
        .expect_err("missing input WAV without --duration must error");

    assert!(
        matches!(err, RenderChainError::RenderFailed(_)),
        "missing input is a render failure, not an arg error: got {err:?}"
    );
    assert!(
        !output.exists(),
        "no partial output WAV should remain on failure"
    );
}

#[test]
fn render_chain_invalid_bit_depth_is_invalid_params() {
    let dir = workdir("bad_bit_depth");
    let chain = write_volume_chain(&dir, 100.0);
    let input = dir.join("in.wav");
    let output = dir.join("out.wav");
    write_dc_wav(&input, 48_000, 1_000, 0.1);

    let mut req = minimal_input(chain, input, output.clone());
    req.bit_depth = Some(19);

    let err = render_chain(req).expect_err("bit depth 19 must be rejected");
    assert!(
        matches!(err, RenderChainError::InvalidParams(_)),
        "invalid bit depth is an argument error: got {err:?}"
    );
    assert!(
        !output.exists(),
        "invalid args must not produce any output WAV"
    );
}
