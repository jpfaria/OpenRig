//! Tests for `resolve_midi_map` — the ADR 0003 precedence (project → system
//! fallback → shipped default), with `input` always taken from the system
//! device profile.

use crate::mapping::{Binding, Source};
use crate::resolve::resolve_midi_map;
use infra_filesystem::midi_profile::MidiDeviceProfile;
use std::fs;
use std::path::PathBuf;

fn tmp_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("openrig_tests_resolve")
        .join(test_name)
        .join(format!("{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create tmp dir");
    dir
}

fn project_binding() -> Binding {
    Binding {
        source: Source::NoteOn {
            channel: 1,
            note: 60,
        },
        command: "SaveProject".to_string(),
        args: serde_json::Value::Null,
        scale: None,
    }
}

fn fallback_yaml() -> &'static str {
    r#"
bindings:
  - source: { kind: note_on, channel: 1, note: 61 }
    command: ApplyRigNav
    args: { chain: "rig:guitar", kind: { StepPreset: 1 } }
"#
}

fn shipped_yaml() -> &'static str {
    r#"
bindings:
  - source: { kind: program_change, program: 0 }
    command: SaveProject
"#
}

#[test]
fn project_bindings_take_precedence_over_fallback_and_default() {
    let dir = tmp_dir("project_wins");
    let fallback = dir.join("midi-bindings.yaml");
    let default = dir.join("midi-map.default.yaml");
    fs::write(&fallback, fallback_yaml()).unwrap();
    fs::write(&default, shipped_yaml()).unwrap();

    let project_bindings = vec![project_binding()];
    let profile = MidiDeviceProfile::default();

    let resolved =
        resolve_midi_map(Some(&project_bindings), &profile, &fallback, &default).unwrap();
    assert_eq!(resolved.bindings.len(), 1);
    assert_eq!(resolved.bindings[0].command, "SaveProject");
    assert_eq!(
        resolved.bindings[0].source,
        Source::NoteOn {
            channel: 1,
            note: 60
        }
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fallback_used_when_project_has_no_bindings() {
    let dir = tmp_dir("fallback_used");
    let fallback = dir.join("midi-bindings.yaml");
    let default = dir.join("midi-map.default.yaml");
    fs::write(&fallback, fallback_yaml()).unwrap();
    fs::write(&default, shipped_yaml()).unwrap();

    let resolved =
        resolve_midi_map(None, &MidiDeviceProfile::default(), &fallback, &default).unwrap();
    assert_eq!(resolved.bindings.len(), 1);
    assert_eq!(
        resolved.bindings[0].source,
        Source::NoteOn {
            channel: 1,
            note: 61
        }
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn shipped_default_used_when_no_project_and_no_fallback() {
    let dir = tmp_dir("default_used");
    let fallback = dir.join("midi-bindings.yaml"); // not created
    let default = dir.join("midi-map.default.yaml");
    fs::write(&default, shipped_yaml()).unwrap();

    let resolved =
        resolve_midi_map(None, &MidiDeviceProfile::default(), &fallback, &default).unwrap();
    assert_eq!(resolved.bindings.len(), 1);
    assert_eq!(
        resolved.bindings[0].source,
        Source::ProgramChange { program: 0 }
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_bindings_when_no_source_exists() {
    let dir = tmp_dir("empty_no_source");
    let fallback = dir.join("midi-bindings.yaml"); // missing
    let default = dir.join("midi-map.default.yaml"); // missing

    let resolved =
        resolve_midi_map(None, &MidiDeviceProfile::default(), &fallback, &default).unwrap();
    assert!(resolved.bindings.is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn input_always_taken_from_device_profile() {
    let dir = tmp_dir("input_from_profile");
    let fallback = dir.join("midi-bindings.yaml");
    let default = dir.join("midi-map.default.yaml");
    fs::write(&fallback, fallback_yaml()).unwrap();
    fs::write(&default, shipped_yaml()).unwrap();

    let profile = MidiDeviceProfile {
        input: Some("Chocolate".into()),
    };

    let project_bindings = vec![project_binding()];
    let resolved =
        resolve_midi_map(Some(&project_bindings), &profile, &fallback, &default).unwrap();
    assert_eq!(resolved.input.as_deref(), Some("Chocolate"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_project_bindings_falls_through_to_fallback() {
    // `Some(&[])` (project carries `midi:` but with empty bindings) is
    // treated as no project layer — fall through. Same as `None`.
    let dir = tmp_dir("empty_project_falls_through");
    let fallback = dir.join("midi-bindings.yaml");
    let default = dir.join("midi-map.default.yaml");
    fs::write(&fallback, fallback_yaml()).unwrap();
    fs::write(&default, shipped_yaml()).unwrap();

    let empty: Vec<Binding> = vec![];
    let resolved = resolve_midi_map(
        Some(&empty),
        &MidiDeviceProfile::default(),
        &fallback,
        &default,
    )
    .unwrap();
    assert_eq!(resolved.bindings.len(), 1);
    // Fallback wins because project bindings is empty.
    assert_eq!(
        resolved.bindings[0].source,
        Source::NoteOn {
            channel: 1,
            note: 61
        }
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_command_in_resolved_map_is_rejected() {
    // Validation runs on the resolved map: an unknown Command name in the
    // fallback file must fail the resolve, not silently drop bindings.
    let dir = tmp_dir("unknown_command_rejected");
    let fallback = dir.join("midi-bindings.yaml");
    let default = dir.join("midi-map.default.yaml");
    fs::write(
        &fallback,
        r#"
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: NotARealCommand
"#,
    )
    .unwrap();
    fs::write(&default, shipped_yaml()).unwrap();

    let err = resolve_midi_map(None, &MidiDeviceProfile::default(), &fallback, &default)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("NotARealCommand") || err.contains("binding #0"),
        "{err}"
    );
    let _ = fs::remove_dir_all(&dir);
}
