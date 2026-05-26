//! Phase 4 red-first test (issue #548): match a raw MIDI message against
//! all active profiles and collect every binding that fires.
//!
//! Contract:
//! - `source:` substring filters the MIDI port name; missing means "any port".
//! - `kind` + `channel` always match by value.
//! - The per-kind value field (`note` / `controller` / `program`) is
//!   exact when present, wildcard when omitted (matches anything).
//! - A message hitting bindings in 2 active profiles produces 2 slot
//!   hits (no exclusivity enforcement — explicit decision per spec).

use adapter_midi::profile::parse_profile_yaml;
use adapter_midi::slots::IncomingMessage;
use adapter_midi::pipeline::{match_message, SlotHit};

#[test]
fn message_matches_program_change_binding_by_exact_value() {
    let profile = parse_profile_yaml(
        r#"
name: "Pad"
source: "FootCtrlPlus"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: prev_preset
"#,
    )
    .unwrap();
    let msg = IncomingMessage::ProgramChange { channel: 1, program: 0 };
    let hits = match_message(&[&profile], "FootCtrlPlus Bluetooth", &msg);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].slot, "prev_preset");
}

#[test]
fn message_does_not_match_wrong_channel() {
    let profile = parse_profile_yaml(
        r#"
name: "Pad"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: prev_preset
"#,
    )
    .unwrap();
    let msg = IncomingMessage::ProgramChange { channel: 2, program: 0 };
    assert!(match_message(&[&profile], "any", &msg).is_empty());
}

#[test]
fn message_does_not_match_wrong_program_value() {
    let profile = parse_profile_yaml(
        r#"
name: "Pad"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 7 }
    do: prev_preset
"#,
    )
    .unwrap();
    let msg = IncomingMessage::ProgramChange { channel: 1, program: 8 };
    assert!(match_message(&[&profile], "any", &msg).is_empty());
}

#[test]
fn wildcard_program_matches_any_value_and_keeps_the_byte() {
    let profile = parse_profile_yaml(
        r#"
name: "Jump"
bindings:
  - when: { kind: ProgramChange, channel: 1 }
    do: jump_preset_n
"#,
    )
    .unwrap();
    let msg = IncomingMessage::ProgramChange { channel: 1, program: 42 };
    let hits = match_message(&[&profile], "FootCtrlPlus", &msg);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].slot, "jump_preset_n");
    // The matched message is passed through so the slot can read its value.
    match hits[0].message {
        IncomingMessage::ProgramChange { program, .. } => assert_eq!(program, 42),
        other => panic!("expected ProgramChange, got {other:?}"),
    }
}

#[test]
fn source_substring_filters_port_name() {
    let chocolate = parse_profile_yaml(
        r#"
name: "Chocolate"
source: "FootCtrlPlus"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: prev_preset
"#,
    )
    .unwrap();

    let msg = IncomingMessage::ProgramChange { channel: 1, program: 0 };
    // Port name contains the substring → matches.
    assert_eq!(
        match_message(&[&chocolate], "FootCtrlPlus Bluetooth", &msg).len(),
        1
    );
    // Different port → no match.
    assert!(match_message(&[&chocolate], "SomeOtherDevice", &msg).is_empty());
}

#[test]
fn missing_source_matches_any_port() {
    let universal = parse_profile_yaml(
        r#"
name: "Universal"
bindings:
  - when: { kind: NoteOn, channel: 1, note: 60 }
    do: toggle_tuner
"#,
    )
    .unwrap();
    let msg = IncomingMessage::NoteOn { channel: 1, note: 60, velocity: 100 };
    assert_eq!(match_message(&[&universal], "anything", &msg).len(), 1);
    assert_eq!(match_message(&[&universal], "totally-different", &msg).len(), 1);
}

#[test]
fn two_active_profiles_both_fire_when_message_matches_both() {
    let a = parse_profile_yaml(
        r#"
name: "A"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: prev_preset
"#,
    )
    .unwrap();
    let b = parse_profile_yaml(
        r#"
name: "B"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 0 }
    do: next_preset
"#,
    )
    .unwrap();
    let msg = IncomingMessage::ProgramChange { channel: 1, program: 0 };
    let hits: Vec<SlotHit> = match_message(&[&a, &b], "any", &msg);
    assert_eq!(hits.len(), 2);
    let slots: Vec<&str> = hits.iter().map(|h| h.slot.as_str()).collect();
    assert!(slots.contains(&"prev_preset"));
    assert!(slots.contains(&"next_preset"));
}

#[test]
fn control_change_matches_by_controller_with_value_passthrough() {
    let profile = parse_profile_yaml(
        r#"
name: "Knob"
bindings:
  - when: { kind: ControlChange, channel: 1, controller: 7 }
    do: chain_volume
"#,
    )
    .unwrap();
    let msg = IncomingMessage::ControlChange { channel: 1, controller: 7, value: 90 };
    let hits = match_message(&[&profile], "any", &msg);
    assert_eq!(hits.len(), 1);
    if let IncomingMessage::ControlChange { value, .. } = hits[0].message {
        assert_eq!(value, 90);
    } else {
        panic!("expected ControlChange in hit");
    }
}
