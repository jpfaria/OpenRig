//! Pure translation: a parsed [`MidiMessage`] + the loaded [`MidiMap`] →
//! a typed `Command`. First matching binding wins. No device, no bridge —
//! the daemon is the only impure layer.

use application::command::Command;

use crate::mapping::{inject, Binding, MidiMap, Source};
use crate::message::MidiMessage;

/// Resolve a message to a command via the first matching binding, or `None`
/// if nothing matches (or, defensively, if the command fails to build —
/// `MidiMap::validate` already rejected such maps at load).
pub fn resolve(map: &MidiMap, msg: &MidiMessage) -> Option<Command> {
    let binding = map.bindings.iter().find(|b| matches(&b.source, msg))?;
    let args = build_args(binding, msg);
    application::command_schema::command_from_variant(&binding.command, args).ok()
}

/// Does this source select this message? Program Change ignores channel
/// (footswitch banks rarely pin a channel); the rest match channel + key.
fn matches(source: &Source, msg: &MidiMessage) -> bool {
    match (source, msg) {
        (
            Source::NoteOn { channel, note },
            MidiMessage::NoteOn {
                channel: c,
                note: n,
                ..
            },
        ) => channel == c && note == n,
        (
            Source::NoteOff { channel, note },
            MidiMessage::NoteOff {
                channel: c,
                note: n,
            },
        ) => channel == c && note == n,
        (
            Source::Cc {
                channel,
                controller,
            },
            MidiMessage::ControlChange {
                channel: c,
                controller: cc,
                ..
            },
        ) => channel == c && controller == cc,
        (Source::ProgramChange { program }, MidiMessage::ProgramChange { program: p, .. }) => {
            program == p
        }
        _ => false,
    }
}

/// Static args, plus — for a continuous source — the live value scaled into
/// the target argument (`scale.into`, default `value`; raw 0..=127 if no
/// scale).
fn build_args(binding: &Binding, msg: &MidiMessage) -> serde_json::Value {
    let mut args = binding.args.clone();
    if let MidiMessage::ControlChange { value, .. } = msg {
        let (key, scaled) = match &binding.scale {
            Some(s) => (s.into.clone(), s.apply(*value)),
            None => ("value".to_string(), f64::from(*value)),
        };
        if let Some(num) = serde_json::Number::from_f64(scaled) {
            inject(&mut args, &key, serde_json::Value::Number(num));
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::Scale;

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
            Some(Command::ToggleBlockEnabled { chain, block }) => {
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
            Some(Command::SetBlockParameterNumber { value, path, .. }) => {
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
            Some(Command::SetBlockParameterNumber { value, .. }) => {
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
            Some(Command::SaveProject)
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
            Some(Command::SaveProject)
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
}
