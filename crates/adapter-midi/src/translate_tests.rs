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

// ── #1007: a Mackie fader (Pitch Bend) drives a mixer strip ────────────────

fn mixer_fader_map() -> MidiMap {
    map(r#"
bindings:
  - source: { kind: pitch_bend, channel: 1 }
    command: SetMixerFader
    args: { strip: "in:0@dev" }
    scale: { min: -60.0, max: 12.0, into: gain_db }
"#)
}

#[test]
fn pitch_bend_top_resolves_to_mixer_fader_max() {
    let cmd = resolve(
        &mixer_fader_map(),
        &MidiMessage::PitchBend {
            channel: 1,
            value: 16383,
        },
    );
    match cmd {
        Some(Command::Mixer(application::command::MixerCommand::SetMixerFader {
            strip,
            gain_db,
        })) => {
            assert_eq!(strip, "in:0@dev");
            assert!((gain_db - 12.0).abs() < 1e-4, "gain_db = {gain_db}");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn pitch_bend_on_other_channel_does_not_match() {
    let cmd = resolve(
        &mixer_fader_map(),
        &MidiMessage::PitchBend {
            channel: 2,
            value: 0,
        },
    );
    assert!(cmd.is_none(), "got {cmd:?}");
}

#[test]
fn pitch_bend_projects_to_pitch_bend_source() {
    assert_eq!(
        message_to_source(&MidiMessage::PitchBend {
            channel: 5,
            value: 100
        }),
        Source::PitchBend { channel: 5 }
    );
}

#[test]
fn a_solo_button_resolves_to_toggle_mixer_solo() {
    let map = map(r#"
bindings:
  - source: { kind: note_on, channel: 1, note: 8 }
    command: ToggleMixerSolo
    args: { strip: "out:0,1@dev" }
"#);
    let cmd = resolve(
        &map,
        &MidiMessage::NoteOn {
            channel: 1,
            note: 8,
            velocity: 127,
        },
    );
    match cmd {
        Some(Command::Mixer(application::command::MixerCommand::ToggleMixerSolo { strip })) => {
            assert_eq!(strip, "out:0,1@dev");
        }
        other => panic!("unexpected: {other:?}"),
    }
}

// ── #1007: a knob drives a chain's OWN fader on one strip ─────────────────

#[test]
fn a_knob_resolves_to_a_chain_mixer_fader() {
    let map = map(r#"
bindings:
  - source: { kind: cc, channel: 1, controller: 20 }
    command: SetChainMixerFader
    args: { chain: "rig:guitar", strip: "out:0,1@dev" }
    scale: { min: -60.0, max: 12.0, into: gain_db }
"#);
    let cmd = resolve(
        &map,
        &MidiMessage::ControlChange {
            channel: 1,
            controller: 20,
            value: 127,
        },
    );
    match cmd {
        Some(Command::Mixer(application::command::MixerCommand::SetChainMixerFader {
            chain,
            strip,
            gain_db,
        })) => {
            assert_eq!(chain.0, "rig:guitar");
            assert_eq!(strip, "out:0,1@dev");
            assert!((gain_db - 12.0).abs() < 1e-4, "gain_db = {gain_db}");
        }
        other => panic!("unexpected: {other:?}"),
    }
}
