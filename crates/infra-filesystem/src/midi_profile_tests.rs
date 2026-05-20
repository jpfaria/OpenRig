//! Tests for `MidiDeviceProfile` — system-level controller profile (ADR 0003 / #499).
//!
//! The profile lives at the per-OS `midi-profile.yaml` path and answers the
//! question "WHICH controller". It is machine-bound and never overridden by
//! the project (bindings are project-owned; device choice is not).

use crate::midi_profile::MidiDeviceProfile;
use crate::FilesystemStorage;
use std::fs;
use std::path::PathBuf;

fn tmp_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("openrig_tests_midi_profile")
        .join(test_name)
        .join(format!("{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create tmp dir");
    dir
}

#[test]
fn default_profile_has_no_input() {
    let p = MidiDeviceProfile::default();
    assert!(p.input.is_none());
}

#[test]
fn profile_with_input_round_trips_through_yaml() {
    let p = MidiDeviceProfile {
        input: Some("Chocolate".into()),
    };
    let yaml = serde_yaml::to_string(&p).unwrap();
    assert!(yaml.contains("input: Chocolate"), "got: {yaml}");

    let back: MidiDeviceProfile = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back, p);
}

#[test]
fn profile_none_input_does_not_emit_key() {
    // None → no `input:` line. Avoids phantom `input: null` in the YAML.
    let p = MidiDeviceProfile::default();
    let yaml = serde_yaml::to_string(&p).unwrap();
    assert!(
        !yaml.contains("input:"),
        "should not emit input key: {yaml}"
    );
}

#[test]
fn load_missing_file_returns_default() {
    // A missing midi-profile.yaml is a valid "use system default input" state.
    let dir = tmp_dir("missing_file_returns_default");
    let path = dir.join("midi-profile.yaml");
    let p = MidiDeviceProfile::load(&path).unwrap();
    assert!(p.input.is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn save_and_load_filesystem_roundtrip() {
    let dir = tmp_dir("save_and_load_filesystem_roundtrip");
    let path = dir.join("midi-profile.yaml");

    let profile = MidiDeviceProfile {
        input: Some("M-Vave Chocolate".into()),
    };
    profile.save(&path).unwrap();

    let loaded = MidiDeviceProfile::load(&path).unwrap();
    assert_eq!(loaded, profile);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn save_creates_parent_directory() {
    let dir = tmp_dir("save_creates_parent_directory");
    let path = dir.join("nested/deep/midi-profile.yaml");

    let profile = MidiDeviceProfile {
        input: Some("Chocolate".into()),
    };
    profile.save(&path).unwrap();

    assert!(path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn deserialize_empty_yaml_uses_default() {
    let p: MidiDeviceProfile = serde_yaml::from_str("{}").unwrap();
    assert!(p.input.is_none());
}

#[test]
fn filesystem_storage_midi_profile_path_uses_per_os_config_dir() {
    let path = FilesystemStorage::midi_profile_path().unwrap();
    assert!(
        path.ends_with("OpenRig/midi-profile.yaml"),
        "unexpected midi profile path: {path:?}"
    );
}
