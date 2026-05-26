//! Red-first tests for `openrig --render` dispatch (issue #552, P6).
//!
//! The GUI binary recognises `--render` and short-circuits to the
//! `adapter-render` headless driver instead of opening a Slint window.
//! Mutual-exclusion with the live-rig flags (`--mcp`, `--midi`,
//! positional/`OPENRIG_PROJECT_PATH` launcher path) is enforced here so
//! the live runtime never spins up alongside an offline render.

use adapter_gui::render_dispatch::{classify_launch, LaunchError, LaunchMode};

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn classify_launch_returns_gui_when_render_flag_absent() {
    let args = argv(&["openrig"]);
    let mode = classify_launch(&args).expect("plain invocation parses");
    assert!(matches!(mode, LaunchMode::Gui));
}

#[test]
fn classify_launch_returns_gui_when_only_project_path_given() {
    let args = argv(&["openrig", "--project", "/tmp/p.openrig"]);
    let mode = classify_launch(&args).expect("project-only invocation parses");
    assert!(matches!(mode, LaunchMode::Gui));
}

#[test]
fn classify_launch_returns_render_when_flag_present_with_required_args() {
    let args = argv(&[
        "openrig",
        "--render",
        "--project", "/tmp/p.openrig",
        "--input",   "/tmp/in.wav",
        "--output",  "/tmp/out.wav",
    ]);
    let mode = classify_launch(&args).expect("render mode parses");
    match mode {
        LaunchMode::Render(r) => {
            assert_eq!(r.project.to_string_lossy(), "/tmp/p.openrig");
            assert_eq!(r.input.to_string_lossy(), "/tmp/in.wav");
            assert_eq!(r.output.to_string_lossy(), "/tmp/out.wav");
        }
        LaunchMode::Gui => panic!("expected Render mode"),
    }
}

#[test]
fn classify_launch_rejects_render_with_mcp() {
    let args = argv(&[
        "openrig",
        "--render",
        "--project", "/tmp/p.openrig",
        "--input",   "/tmp/in.wav",
        "--output",  "/tmp/out.wav",
        "--mcp",
    ]);
    let err = classify_launch(&args).expect_err("--render + --mcp must error");
    assert!(matches!(err, LaunchError::MutuallyExclusive(ref f) if f == "--mcp"));
}

#[test]
fn classify_launch_rejects_render_with_midi() {
    let args = argv(&[
        "openrig",
        "--render",
        "--project", "/tmp/p.openrig",
        "--input",   "/tmp/in.wav",
        "--output",  "/tmp/out.wav",
        "--midi",
    ]);
    let err = classify_launch(&args).expect_err("--render + --midi must error");
    assert!(matches!(err, LaunchError::MutuallyExclusive(ref f) if f == "--midi"));
}

#[test]
fn classify_launch_rejects_render_with_positional_project() {
    let args = argv(&[
        "openrig",
        "/tmp/legacy_positional.openrig",
        "--render",
        "--project", "/tmp/p.openrig",
        "--input",   "/tmp/in.wav",
        "--output",  "/tmp/out.wav",
    ]);
    let err = classify_launch(&args).expect_err("--render + positional path must error");
    assert!(matches!(
        err,
        LaunchError::MutuallyExclusive(ref f) if f == "<positional project path>"
    ));
}

#[test]
fn classify_launch_propagates_render_args_errors() {
    let args = argv(&["openrig", "--render", "--project", "/tmp/p.openrig"]);
    let err = classify_launch(&args).expect_err("missing --input/--output must error");
    assert!(matches!(err, LaunchError::RenderArgs(_)));
}
