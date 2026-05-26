//! Red-first (issue #548): load every `*.yaml` profile from a directory
//! at runtime so users drop their own files into
//! `~/.local/share/openrig/midi-profiles/` (or the install assets dir)
//! and the daemon picks them up — no rebuild.

use adapter_midi::profile::load_profiles_from_dir;

#[test]
fn loads_every_yaml_in_the_dir() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("a.yaml"),
        r#"
name: "A"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: prev_preset
"#,
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("b.yaml"),
        r#"
name: "B"
bindings:
  - when: { kind: NoteOn, channel: 1, note: 60 }
    do: toggle_tuner
"#,
    )
    .unwrap();
    // Non-YAML file should be ignored.
    std::fs::write(tmp.path().join("readme.md"), "not yaml").unwrap();

    let profiles = load_profiles_from_dir(tmp.path());
    let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(profiles.len(), 2, "got names: {names:?}");
    assert!(names.contains(&"A"));
    assert!(names.contains(&"B"));
}

#[test]
fn missing_dir_returns_empty() {
    let nope = std::path::Path::new("/tmp/this/dir/does/not/exist-548");
    assert!(load_profiles_from_dir(nope).is_empty());
}

#[test]
fn malformed_yaml_is_skipped_not_panic() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("good.yaml"), "name: \"OK\"\nbindings: []\n").unwrap();
    std::fs::write(tmp.path().join("bad.yaml"), "this: is: not: valid: yaml: : :").unwrap();
    let profiles = load_profiles_from_dir(tmp.path());
    let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(profiles.len(), 1, "got names: {names:?}");
    assert_eq!(names[0], "OK");
}
