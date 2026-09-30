use super::*;

#[test]
fn parses_note_on_with_channel_one_indexed() {
    // 0x90 = Note On, channel 0 on the wire → channel 1 for the user.
    assert_eq!(
        MidiMessage::parse(&[0x90, 60, 100]),
        Some(MidiMessage::NoteOn {
            channel: 1,
            note: 60,
            velocity: 100
        })
    );
}

#[test]
fn parses_note_on_velocity_zero_as_note_off() {
    assert_eq!(
        MidiMessage::parse(&[0x95, 60, 0]),
        Some(MidiMessage::NoteOff {
            channel: 6,
            note: 60
        })
    );
}

#[test]
fn parses_note_off() {
    assert_eq!(
        MidiMessage::parse(&[0x82, 64, 40]),
        Some(MidiMessage::NoteOff {
            channel: 3,
            note: 64
        })
    );
}

#[test]
fn parses_control_change() {
    assert_eq!(
        MidiMessage::parse(&[0xB0, 7, 127]),
        Some(MidiMessage::ControlChange {
            channel: 1,
            controller: 7,
            value: 127
        })
    );
}

#[test]
fn parses_program_change_two_bytes() {
    assert_eq!(
        MidiMessage::parse(&[0xC3, 5]),
        Some(MidiMessage::ProgramChange {
            channel: 4,
            program: 5
        })
    );
}

#[test]
fn rejects_system_realtime() {
    assert_eq!(MidiMessage::parse(&[0xF8]), None);
    assert_eq!(MidiMessage::parse(&[0xFA]), None);
}

#[test]
fn rejects_truncated_message() {
    assert_eq!(MidiMessage::parse(&[0x90, 60]), None);
    assert_eq!(MidiMessage::parse(&[]), None);
}

#[test]
fn rejects_unsupported_channel_voice() {
    // 0xA0 = polyphonic aftertouch — not a bindable source.
    assert_eq!(MidiMessage::parse(&[0xA0, 60, 10]), None);
}

// ── #1007: Pitch Bend carries a Mackie fader's 14-bit position ─────────────

#[test]
fn parses_pitch_bend_as_fourteen_bit_value() {
    // LSB first, then MSB: centre is 0x2000.
    assert_eq!(
        MidiMessage::parse(&[0xE0, 0x00, 0x40]),
        Some(MidiMessage::PitchBend {
            channel: 1,
            value: 8192
        })
    );
    assert_eq!(
        MidiMessage::parse(&[0xE3, 0x7F, 0x7F]),
        Some(MidiMessage::PitchBend {
            channel: 4,
            value: 16383
        })
    );
}

#[test]
fn rejects_truncated_pitch_bend() {
    assert_eq!(MidiMessage::parse(&[0xE0, 0x00]), None);
}
