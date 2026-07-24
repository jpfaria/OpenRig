use super::*;
use crate::mapping::Scale;
use application::command::{BlockCommand, ProjectCommand};

fn map(yaml: &str) -> MidiMap {
    serde_yaml::from_str(yaml).unwrap()
}

#[test]
fn note_on_resolves_to_discrete_command() {
    let m = map(r#"
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: ToggleBlockEnabled
    args: { chain: "chain:a", block: "block:b" }
"#);
    let cmd = resolve(
        &m,
        &MidiMessage::NoteOn {
            channel: 1,
            note: 60,
            velocity: 100,
        },
    );
    match cmd {
        Some(Command::Block(BlockCommand::ToggleBlockEnabled { chain, block })) => {
            assert_eq!(chain.0, "chain:a");
            assert_eq!(block.0, "block:b");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn non_matching_channel_yields_none() {
    let m = map(r#"
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: SaveProject
"#);
    assert!(resolve(
        &m,
        &MidiMessage::NoteOn {
            channel: 2,
            note: 60,
            velocity: 100
        }
    )
    .is_none());
}

#[test]
fn cc_value_is_scaled_into_command_argument() {
    let m = map(r#"
bindings:
  - source: { kind: cc, channel: 1, controller: 7 }
    command: SetBlockParameterNumber
    args: { chain: "chain:a", block: "block:b", path: gain }
    scale: { min: 0.0, max: 100.0 }
"#);
    let cmd = resolve(
        &m,
        &MidiMessage::ControlChange {
            channel: 1,
            controller: 7,
            value: 127,
        },
    );
    match cmd {
        Some(Command::Block(BlockCommand::SetBlockParameterNumber { value, path, .. })) => {
            assert_eq!(path, "gain");
            assert!((value - 100.0).abs() < 1e-9);
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn cc_without_scale_passes_raw_value() {
    let m = map(r#"
bindings:
  - source: { kind: cc, channel: 1, controller: 7 }
    command: SetBlockParameterNumber
    args: { chain: "chain:a", block: "block:b", path: gain }
"#);
    let cmd = resolve(
        &m,
        &MidiMessage::ControlChange {
            channel: 1,
            controller: 7,
            value: 64,
        },
    );
    match cmd {
        Some(Command::Block(BlockCommand::SetBlockParameterNumber { value, .. })) => {
            assert!((value - 64.0).abs() < 1e-9);
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn program_change_matches_any_channel() {
    let m = map(r#"
bindings:
  - source: { kind: program_change, program: 5 }
    command: SaveProject
"#);
    assert!(matches!(
        resolve(
            &m,
            &MidiMessage::ProgramChange {
                channel: 9,
                program: 5
            }
        ),
        Some(Command::Project(ProjectCommand::SaveProject))
    ));
}

#[test]
fn first_matching_binding_wins() {
    let m = map(r#"
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: SaveProject
  - source: { kind: note_on, channel: 1, note: 60 }
    command: UpdateProjectName
    args: { name: second }
"#);
    assert!(matches!(
        resolve(
            &m,
            &MidiMessage::NoteOn {
                channel: 1,
                note: 60,
                velocity: 1
            }
        ),
        Some(Command::Project(ProjectCommand::SaveProject))
    ));
}

#[test]
fn scale_into_custom_key_is_respected() {
    let s = Scale {
        min: 0.0,
        max: 1.0,
        into: "value".into(),
    };
    assert!((s.apply(127) - 1.0).abs() < 1e-9);
}

#[test]
fn source_from_bytes_projects_note_on() {
    assert_eq!(
        source_from_bytes(&[0x90, 60, 100]),
        Some(Source::NoteOn {
            channel: 1,
            note: 60
        })
    );
}

#[test]
fn source_from_bytes_projects_cc_dropping_value() {
    // Two CCs on the same controller produce the SAME Source — the
    // learn editor binds the controller, not the live value.
    let a = source_from_bytes(&[0xB0, 7, 0]).unwrap();
    let b = source_from_bytes(&[0xB0, 7, 127]).unwrap();
    assert_eq!(a, b);
    assert_eq!(
        a,
        Source::Cc {
            channel: 1,
            controller: 7
        }
    );
}

#[test]
fn source_from_bytes_program_change_ignores_channel() {
    // PC bindings ignore channel (see `matches`), so the projected
    // source mirrors that — same `Source` for the same program no
    // matter which channel sent it.
    let a = source_from_bytes(&[0xC0, 5]).unwrap();
    let b = source_from_bytes(&[0xC9, 5]).unwrap();
    assert_eq!(a, b);
    assert_eq!(a, Source::ProgramChange { program: 5 });
}

#[test]
fn source_from_bytes_rejects_unbindable() {
    assert!(source_from_bytes(&[]).is_none());
    assert!(source_from_bytes(&[0xF8]).is_none()); // system real-time
    assert!(source_from_bytes(&[0x90, 60]).is_none()); // truncated
}

#[test]
fn source_from_bytes_note_on_velocity_zero_becomes_note_off() {
    // Matches MidiMessage::parse's running-status convention.
    assert_eq!(
        source_from_bytes(&[0x90, 60, 0]),
        Some(Source::NoteOff {
            channel: 1,
            note: 60
        })
    );
}
