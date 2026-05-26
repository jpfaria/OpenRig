//! Red-first tests for `RenderArgs` CLI parsing (issue #552).

use adapter_render::cli::{parse_render_args, RenderArgs, RenderArgsError};

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn parse_render_args_with_minimal_required_flags() {
    let args = argv(&[
        "openrig-render",
        "--project", "/tmp/p.openrig",
        "--input",   "/tmp/in.wav",
        "--output",  "/tmp/out.wav",
    ]);
    let parsed: RenderArgs = parse_render_args(&args).expect("required flags parse");
    assert_eq!(parsed.project.to_string_lossy(), "/tmp/p.openrig");
    assert_eq!(parsed.input.to_string_lossy(),   "/tmp/in.wav");
    assert_eq!(parsed.output.to_string_lossy(),  "/tmp/out.wav");
    assert_eq!(parsed.chain, None);
    assert_eq!(parsed.sample_rate_hz, 48_000);
    assert_eq!(parsed.block_size, 256);
    assert_eq!(parsed.bit_depth, 24);
    assert_eq!(parsed.tail_ms, 2_000);
}

#[test]
fn parse_render_args_with_all_optional_flags() {
    let args = argv(&[
        "openrig-render",
        "--project",     "/tmp/p.openrig",
        "--input",       "/tmp/in.wav",
        "--output",      "/tmp/out.wav",
        "--chain",       "lead",
        "--sample-rate", "44100",
        "--block-size",  "512",
        "--bit-depth",   "32",
        "--tail-ms",     "3500",
    ]);
    let parsed = parse_render_args(&args).expect("all flags parse");
    assert_eq!(parsed.chain.as_deref(), Some("lead"));
    assert_eq!(parsed.sample_rate_hz, 44_100);
    assert_eq!(parsed.block_size, 512);
    assert_eq!(parsed.bit_depth, 32);
    assert_eq!(parsed.tail_ms, 3_500);
}

#[test]
fn parse_render_args_rejects_missing_project() {
    let args = argv(&[
        "openrig-render",
        "--input",  "/tmp/in.wav",
        "--output", "/tmp/out.wav",
    ]);
    let err = parse_render_args(&args).expect_err("missing --project must error");
    assert!(
        matches!(err, RenderArgsError::MissingRequired(ref f) if f == "--project"),
        "expected MissingRequired(--project), got {err:?}"
    );
}

#[test]
fn parse_render_args_rejects_missing_input() {
    let args = argv(&[
        "openrig-render",
        "--project", "/tmp/p.openrig",
        "--output",  "/tmp/out.wav",
    ]);
    let err = parse_render_args(&args).expect_err("missing --input must error");
    assert!(matches!(err, RenderArgsError::MissingRequired(ref f) if f == "--input"));
}

#[test]
fn parse_render_args_rejects_missing_output() {
    let args = argv(&[
        "openrig-render",
        "--project", "/tmp/p.openrig",
        "--input",   "/tmp/in.wav",
    ]);
    let err = parse_render_args(&args).expect_err("missing --output must error");
    assert!(matches!(err, RenderArgsError::MissingRequired(ref f) if f == "--output"));
}

#[test]
fn parse_render_args_rejects_unknown_flag() {
    let args = argv(&[
        "openrig-render",
        "--project", "/tmp/p.openrig",
        "--input",   "/tmp/in.wav",
        "--output",  "/tmp/out.wav",
        "--bogus",   "x",
    ]);
    let err = parse_render_args(&args).expect_err("unknown flag must error");
    assert!(matches!(err, RenderArgsError::UnknownFlag(ref f) if f == "--bogus"));
}

#[test]
fn parse_render_args_rejects_flag_without_value() {
    let args = argv(&[
        "openrig-render",
        "--project",
    ]);
    let err = parse_render_args(&args).expect_err("dangling flag must error");
    assert!(matches!(err, RenderArgsError::MissingValue(ref f) if f == "--project"));
}

#[test]
fn parse_render_args_rejects_invalid_bit_depth() {
    let args = argv(&[
        "openrig-render",
        "--project",   "/tmp/p.openrig",
        "--input",     "/tmp/in.wav",
        "--output",    "/tmp/out.wav",
        "--bit-depth", "19",
    ]);
    let err = parse_render_args(&args).expect_err("invalid bit depth must error");
    assert!(matches!(err, RenderArgsError::InvalidValue { .. }));
}

#[test]
fn parse_render_args_rejects_mutual_exclusion_with_mcp() {
    let args = argv(&[
        "openrig-render",
        "--project", "/tmp/p.openrig",
        "--input",   "/tmp/in.wav",
        "--output",  "/tmp/out.wav",
        "--mcp",
    ]);
    let err = parse_render_args(&args).expect_err("--render incompatible with --mcp");
    assert!(matches!(err, RenderArgsError::MutuallyExclusive(ref f) if f == "--mcp"));
}

#[test]
fn parse_render_args_rejects_mutual_exclusion_with_midi() {
    let args = argv(&[
        "openrig-render",
        "--project", "/tmp/p.openrig",
        "--input",   "/tmp/in.wav",
        "--output",  "/tmp/out.wav",
        "--midi",
    ]);
    let err = parse_render_args(&args).expect_err("--render incompatible with --midi");
    assert!(matches!(err, RenderArgsError::MutuallyExclusive(ref f) if f == "--midi"));
}
