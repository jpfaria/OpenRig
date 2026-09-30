//! Responsibility: derives the MIDI bytes that show a mixer strip's state on a surface.
//! #1007 — a controller that moves a mixer strip should also *show* it: the
//! motor fader follows a change made in the GUI or over MCP, the MUTE LED
//! lights. The bytes come from the same bindings that read the surface, so
//! any controller works without device-specific code: a Mackie Control
//! surface (Pitch Bend faders, Note On mutes) or a plain CC fader box.
//!
//! Pure: no device, no `midir`. The caller sends the bytes.

use serde_json::Value;

use crate::mapping::{Binding, Source};
use crate::message::PITCH_BEND_MAX;

const SET_FADER: &str = "SetMixerFader";
const MUTE_COMMANDS: [&str; 2] = ["ToggleMixerMute", "SetMixerMute"];

/// Every message a controller needs to mirror `strip`: fader position for
/// its fader bindings, LED state for its mute bindings. Bindings for other
/// strips or other commands contribute nothing.
pub fn strip_feedback(
    bindings: &[Binding],
    strip: &str,
    gain_db: f32,
    muted: bool,
) -> Vec<[u8; 3]> {
    bindings
        .iter()
        .filter(|b| targets_strip(b, strip))
        .filter_map(|b| binding_feedback(b, gain_db, muted))
        .collect()
}

fn targets_strip(binding: &Binding, strip: &str) -> bool {
    binding.args.get("strip").and_then(Value::as_str) == Some(strip)
}

fn binding_feedback(binding: &Binding, gain_db: f32, muted: bool) -> Option<[u8; 3]> {
    if binding.command == SET_FADER {
        let t = binding.scale.as_ref()?.unit_of(f64::from(gain_db));
        return fader_position(&binding.source, t);
    }
    if MUTE_COMMANDS.contains(&binding.command.as_str()) {
        return mute_led(&binding.source, muted);
    }
    None
}

/// `t` is the `0..=1` travel position.
fn fader_position(source: &Source, t: f64) -> Option<[u8; 3]> {
    match *source {
        Source::PitchBend { channel } => {
            let value = (t * f64::from(PITCH_BEND_MAX)).round() as u16;
            Some([
                0xE0 | wire_channel(channel),
                (value & 0x7F) as u8,
                (value >> 7) as u8,
            ])
        }
        Source::Cc {
            channel,
            controller,
        } => Some([
            0xB0 | wire_channel(channel),
            controller & 0x7F,
            (t * 127.0).round() as u8,
        ]),
        _ => None,
    }
}

fn mute_led(source: &Source, muted: bool) -> Option<[u8; 3]> {
    match *source {
        Source::NoteOn { channel, note } => Some([
            0x90 | wire_channel(channel),
            note & 0x7F,
            if muted { 127 } else { 0 },
        ]),
        _ => None,
    }
}

/// Bindings number channels 1..=16; the wire uses 0..=15.
fn wire_channel(channel: u8) -> u8 {
    channel.saturating_sub(1) & 0x0F
}

#[cfg(test)]
#[path = "mixer_feedback_tests.rs"]
mod tests;
