//! Red-first tests for `RenderArgs` CLI parsing (issue #552).

use adapter_render::cli::{parse_render_args, RenderArgs, RenderArgsError};

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn parse_render_args_with_minimal_required_flags() {
    let args = argv(&[
        "openrig-render",
        "--chain",
        "/tmp/c.yaml",
        "--input",
        "/tmp/in.wav",
        "--output",
        "/tmp/out.wav",
    ]);
    let parsed: RenderArgs = parse_render_args(&args).expect("required flags parse");
    assert_eq!(parsed.chain.to_string_lossy(), "/tmp/c.yaml");
    assert_eq!(parsed.input.to_string_lossy(), "/tmp/in.wav");
    assert_eq!(parsed.output.to_string_lossy(), "/tmp/out.wav");
    assert_eq!(parsed.sample_rate_hz, 48_000);
    assert_eq!(parsed.block_size, 256);
    assert_eq!(parsed.bit_depth, 24);
    assert_eq!(parsed.tail_ms, 2_000);
    assert_eq!(parsed.start_s, None);
    assert_eq!(parsed.end_s, None);
    assert_eq!(parsed.duration_s, None);
    assert_eq!(parsed.input_device, None);
}

#[test]
fn parse_render_args_with_slice() {
    let args = argv(&[
        "openrig-render",
        "--chain",
        "/tmp/c.yaml",
        "--input",
        "/tmp/in.wav",
        "--output",
        "/tmp/out.wav",
        "--start",
        "5.0",
        "--end",
        "15.5",
    ]);
    let parsed = parse_render_args(&args).expect("slice parses");
    assert_eq!(parsed.start_s, Some(5.0));
    assert_eq!(parsed.end_s, Some(15.5));
}

#[test]
fn parse_render_args_with_capture_flags() {
    let args = argv(&[
        "openrig-render",
        "--chain",
        "/tmp/c.yaml",
        "--input",
        "/tmp/di.wav",
        "--output",
        "/tmp/wet.wav",
        "--duration",
        "10",
        "--input-device",
        "Focusrite Scarlett 2i2",
    ]);
    let parsed = parse_render_args(&args).expect("capture flags parse");
    assert_eq!(parsed.duration_s, Some(10.0));
    assert_eq!(
        parsed.input_device.as_deref(),
        Some("Focusrite Scarlett 2i2")
    );
}

#[test]
fn parse_render_args_rejects_missing_chain() {
    let args = argv(&[
        "openrig-render",
        "--input",
        "/tmp/in.wav",
        "--output",
        "/tmp/out.wav",
    ]);
    let err = parse_render_args(&args).expect_err("missing --chain must error");
    assert!(matches!(err, RenderArgsError::MissingRequired(ref f) if f == "--chain"));
}

#[test]
fn parse_render_args_rejects_missing_input() {
    let args = argv(&[
        "openrig-render",
        "--chain",
        "/tmp/c.yaml",
        "--output",
        "/tmp/out.wav",
    ]);
    let err = parse_render_args(&args).expect_err("missing --input must error");
    assert!(matches!(err, RenderArgsError::MissingRequired(ref f) if f == "--input"));
}

#[test]
fn parse_render_args_rejects_missing_output() {
    let args = argv(&[
        "openrig-render",
        "--chain",
        "/tmp/c.yaml",
        "--input",
        "/tmp/in.wav",
    ]);
    let err = parse_render_args(&args).expect_err("missing --output must error");
    assert!(matches!(err, RenderArgsError::MissingRequired(ref f) if f == "--output"));
}

#[test]
fn parse_render_args_rejects_unknown_flag() {
    let args = argv(&[
        "openrig-render",
        "--chain",
        "/tmp/c.yaml",
        "--input",
        "/tmp/in.wav",
        "--output",
        "/tmp/out.wav",
        "--bogus",
        "x",
    ]);
    let err = parse_render_args(&args).expect_err("unknown flag must error");
    assert!(matches!(err, RenderArgsError::UnknownFlag(ref f) if f == "--bogus"));
}

#[test]
fn parse_render_args_rejects_flag_without_value() {
    let args = argv(&["openrig-render", "--chain"]);
    let err = parse_render_args(&args).expect_err("dangling flag must error");
    assert!(matches!(err, RenderArgsError::MissingValue(ref f) if f == "--chain"));
}

#[test]
fn parse_render_args_rejects_invalid_bit_depth() {
    let args = argv(&[
        "openrig-render",
        "--chain",
        "/tmp/c.yaml",
        "--input",
        "/tmp/in.wav",
        "--output",
        "/tmp/out.wav",
        "--bit-depth",
        "19",
    ]);
    let err = parse_render_args(&args).expect_err("invalid bit depth must error");
    assert!(matches!(err, RenderArgsError::InvalidValue { .. }));
}
