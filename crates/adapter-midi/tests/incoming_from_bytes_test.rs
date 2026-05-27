//! Phase 5 wiring red-first: parse raw MIDI bytes coming out of `midir`
//! into `IncomingMessage`. Needed by the daemon callback that will plug
//! `pipeline::dispatch_midi_message` into the live MIDI stream.
//!
//! MIDI 1.0 status byte encoding (high nibble = type, low nibble =
//! channel 0-15 → expose as 1-16):
//!   0x80 NoteOff   data1=note  data2=velocity
//!   0x90 NoteOn    data1=note  data2=velocity (vel 0 = NoteOff)
//!   0xB0 CC        data1=ctrl  data2=value
//!   0xC0 PC        data1=program

use adapter_midi::slots::IncomingMessage;

#[test]
fn parses_note_on() {
    let bytes = [0x90, 60, 100];
    let msg = IncomingMessage::from_bytes(&bytes).unwrap();
    assert_eq!(
        msg,
        IncomingMessage::NoteOn {
            channel: 1,
            note: 60,
            velocity: 100
        }
    );
}

#[test]
fn parses_note_on_channel_16() {
    let bytes = [0x9F, 60, 100];
    let msg = IncomingMessage::from_bytes(&bytes).unwrap();
    assert!(matches!(msg, IncomingMessage::NoteOn { channel: 16, .. }));
}

#[test]
fn note_on_with_velocity_zero_is_note_off() {
    let bytes = [0x90, 60, 0];
    let msg = IncomingMessage::from_bytes(&bytes).unwrap();
    assert_eq!(
        msg,
        IncomingMessage::NoteOff {
            channel: 1,
            note: 60
        }
    );
}

#[test]
fn parses_note_off() {
    let bytes = [0x80, 60, 64];
    let msg = IncomingMessage::from_bytes(&bytes).unwrap();
    assert_eq!(
        msg,
        IncomingMessage::NoteOff {
            channel: 1,
            note: 60
        }
    );
}

#[test]
fn parses_control_change() {
    let bytes = [0xB0, 7, 90];
    let msg = IncomingMessage::from_bytes(&bytes).unwrap();
    assert_eq!(
        msg,
        IncomingMessage::ControlChange {
            channel: 1,
            controller: 7,
            value: 90
        }
    );
}

#[test]
fn parses_program_change() {
    let bytes = [0xC0, 42];
    let msg = IncomingMessage::from_bytes(&bytes).unwrap();
    assert_eq!(
        msg,
        IncomingMessage::ProgramChange {
            channel: 1,
            program: 42
        }
    );
}

#[test]
fn rejects_empty_bytes() {
    assert!(IncomingMessage::from_bytes(&[]).is_none());
}

#[test]
fn rejects_unsupported_status_byte() {
    // Polyphonic aftertouch (0xA0) — not in the V1 supported set.
    assert!(IncomingMessage::from_bytes(&[0xA0, 60, 80]).is_none());
    // System exclusive — out of scope.
    assert!(IncomingMessage::from_bytes(&[0xF0, 0x7E]).is_none());
}

#[test]
fn rejects_truncated_message() {
    // CC needs 3 bytes
    assert!(IncomingMessage::from_bytes(&[0xB0, 7]).is_none());
    // PC needs 2 bytes
    assert!(IncomingMessage::from_bytes(&[0xC0]).is_none());
}
