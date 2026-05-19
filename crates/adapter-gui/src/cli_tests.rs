//! Tests for the `--project` flag + path validation (#452).

use super::*;
use std::path::PathBuf;

#[test]
fn project_flag_takes_next_arg_as_path() {
    let (p, _, _) = parse_cli_args_from(&["openrig", "--project", "/tmp/x.openrig"]);
    assert_eq!(p, Some(PathBuf::from("/tmp/x.openrig")));
}

#[test]
fn project_flag_overrides_earlier_positional() {
    let (p, _, _) = parse_cli_args_from(&["openrig", "old.yaml", "--project", "new.openrig"]);
    assert_eq!(p, Some(PathBuf::from("new.openrig")));
}

#[test]
fn project_flag_without_value_is_ignored_no_panic() {
    let (p, a, _) = parse_cli_args_from(&["openrig", "--project"]);
    assert_eq!(p, None);
    assert!(!a);
}

#[test]
fn project_flag_coexists_with_auto_save() {
    let (p, a, f) = parse_cli_args_from(&[
        "openrig",
        "--auto-save",
        "--project",
        "r.openrig",
        "--fullscreen",
    ]);
    assert_eq!(p, Some(PathBuf::from("r.openrig")));
    assert!(a);
    assert!(f);
}

#[test]
fn validate_missing_path_is_clear_error() {
    let err = validate_project_path(std::path::Path::new("/no/such/openrig/project.openrig"))
        .unwrap_err();
    assert!(err.contains("not found"), "got: {err}");
    assert!(err.contains("project.openrig"), "names the path: {err}");
}

#[test]
fn validate_directory_is_not_a_file_error() {
    let dir = tempfile::tempdir().unwrap();
    let err = validate_project_path(dir.path()).unwrap_err();
    assert!(err.contains("not a file"), "got: {err}");
}

#[test]
fn validate_existing_file_ok() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.openrig");
    std::fs::write(&path, "project:\n  name: x\n").unwrap();
    assert!(validate_project_path(&path).is_ok());
}

#[test]
fn mcp_flag_bare_uses_default_addr() {
    let addr = parse_mcp_addr(&["openrig", "--mcp"]);
    assert_eq!(addr, Some("127.0.0.1:4123".parse().unwrap()));
}

#[test]
fn mcp_flag_with_value_parses_addr() {
    let addr = parse_mcp_addr(&["openrig", "--mcp=0.0.0.0:9000"]);
    assert_eq!(addr, Some("0.0.0.0:9000".parse().unwrap()));
}

#[test]
fn mcp_flag_absent_is_none() {
    assert_eq!(parse_mcp_addr(&["openrig", "/tmp/p.openrig"]), None);
}

#[test]
fn mcp_flag_invalid_value_is_none() {
    assert_eq!(parse_mcp_addr(&["openrig", "--mcp=not-an-addr"]), None);
}

#[test]
fn midi_flag_bare_is_default() {
    assert_eq!(
        parse_midi_map(&["openrig", "--midi"]),
        Some(MidiMapArg::Default)
    );
}

#[test]
fn midi_flag_with_path_is_explicit() {
    assert_eq!(
        parse_midi_map(&["openrig", "--midi=/tmp/m.yaml"]),
        Some(MidiMapArg::Path("/tmp/m.yaml".into()))
    );
}

#[test]
fn midi_flag_absent_is_none() {
    assert_eq!(parse_midi_map(&["openrig", "/tmp/p.openrig"]), None);
}
