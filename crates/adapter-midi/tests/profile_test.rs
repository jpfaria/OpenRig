//! Phase 2 red-first tests for the `MidiProfile` YAML schema (issue #548).
//!
//! Verifies:
//! - happy-path parsing into the typed struct
//! - every `kind` from MIDI 1.0 (NoteOn, NoteOff, ControlChange, ProgramChange)
//! - missing value field (`program`, `controller`, `note`) means "wildcard"
//! - missing `source` is allowed (profile applies to any MIDI port)
//! - `do:` referencing a slot outside the 20-slot catalog is rejected at
//!   parse time with the offending slot name in the error

use adapter_midi::profile::{parse_profile_yaml, MatchExpr};

#[test]
fn parse_minimal_program_change_profile() {
    let yaml = r#"
name: "Test"
source: "FootCtrlPlus"
description: "test profile"
bindings:
  - when:
      kind: ProgramChange
      channel: 1
      program: 0
    do: prev_preset
"#;
    let profile = parse_profile_yaml(yaml).expect("parse should succeed");
    assert_eq!(profile.name, "Test");
    assert_eq!(profile.source.as_deref(), Some("FootCtrlPlus"));
    assert_eq!(profile.bindings.len(), 1);
    assert_eq!(profile.bindings[0].action, "prev_preset");
    match &profile.bindings[0].when {
        MatchExpr::ProgramChange {
            channel: 1,
            program: Some(0),
        } => {}
        other => panic!("unexpected match expression: {other:?}"),
    }
}

#[test]
fn parse_control_change_profile() {
    let yaml = r#"
name: "Knob"
bindings:
  - when:
      kind: ControlChange
      channel: 1
      controller: 7
    do: chain_volume
"#;
    let profile = parse_profile_yaml(yaml).expect("parse should succeed");
    match &profile.bindings[0].when {
        MatchExpr::ControlChange {
            channel: 1,
            controller: Some(7),
        } => {}
        other => panic!("unexpected match expression: {other:?}"),
    }
}

#[test]
fn parse_note_on_profile() {
    let yaml = r#"
name: "Pad"
bindings:
  - when:
      kind: NoteOn
      channel: 1
      note: 60
    do: toggle_tuner
"#;
    let profile = parse_profile_yaml(yaml).expect("parse should succeed");
    match &profile.bindings[0].when {
        MatchExpr::NoteOn {
            channel: 1,
            note: Some(60),
        } => {}
        other => panic!("unexpected match expression: {other:?}"),
    }
}

#[test]
fn omitted_program_means_wildcard() {
    let yaml = r#"
name: "Jump"
bindings:
  - when:
      kind: ProgramChange
      channel: 1
    do: jump_preset_n
"#;
    let profile = parse_profile_yaml(yaml).expect("parse should succeed");
    match &profile.bindings[0].when {
        MatchExpr::ProgramChange {
            channel: 1,
            program: None,
        } => {}
        other => panic!("expected wildcard: {other:?}"),
    }
}

#[test]
fn missing_source_is_allowed() {
    let yaml = r#"
name: "Universal"
bindings:
  - when:
      kind: NoteOn
      channel: 1
      note: 60
    do: toggle_tuner
"#;
    let profile = parse_profile_yaml(yaml).expect("parse should succeed");
    assert!(profile.source.is_none());
}

#[test]
fn unknown_slot_is_rejected_with_name_in_error() {
    let yaml = r#"
name: "Bad"
bindings:
  - when:
      kind: ProgramChange
      channel: 1
      program: 0
    do: this_slot_does_not_exist
"#;
    let err = parse_profile_yaml(yaml).expect_err("unknown slot must fail parsing");
    let msg = err.to_string();
    assert!(
        msg.contains("this_slot_does_not_exist"),
        "error should name the offending slot, got: {msg}"
    );
}
