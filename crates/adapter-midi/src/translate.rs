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

/// Project a parsed [`MidiMessage`] onto a [`Source`] descriptor — i.e. drop
/// the live `velocity` / `value` / channel-on-PC fields so the result is the
/// same shape `Binding::source` carries. Used by the learn-mode path in
/// [`crate::daemon`] (#513 / #493): while learn-mode is active, every
/// incoming event is published as `MidiCommand::PublishMidiEvent { source }`
/// regardless of whether the binding map already has a row for it.
pub fn message_to_source(msg: &MidiMessage) -> Source {
    match *msg {
        MidiMessage::NoteOn { channel, note, .. } => Source::NoteOn { channel, note },
        MidiMessage::NoteOff { channel, note } => Source::NoteOff { channel, note },
        MidiMessage::ControlChange {
            channel,
            controller,
            ..
        } => Source::Cc {
            channel,
            controller,
        },
        // Program Change bindings ignore channel (see `matches` above), so
        // the projected source mirrors that — the editor learns "PC #5", not
        // "PC #5 on channel 4".
        MidiMessage::ProgramChange { program, .. } => Source::ProgramChange { program },
    }
}

/// Parse raw `midir` bytes into a [`Source`] in one step. Returns `None` for
/// system / real-time / truncated / unsupported messages (same set
/// [`MidiMessage::parse`] rejects). Shared by the daemon's learn-mode path
/// so both the publish and the auto-disarm decisions agree on what counts
/// as a bindable event.
pub fn source_from_bytes(bytes: &[u8]) -> Option<Source> {
    MidiMessage::parse(bytes).map(|m| message_to_source(&m))
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
#[path = "translate_tests.rs"]
mod tests;
