//! Parse raw MIDI bytes into the small set of channel-voice messages a
//! controller binding can target. Pure and fully testable — no device, no
//! `midir`. System/real-time/unsupported messages parse to `None`.

/// A channel-voice MIDI message. `channel` is 1..=16 (human/`midi-map.yaml`
/// numbering), not the 0..=15 wire value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
}

impl MidiMessage {
    /// Parse one message from a raw `midir` byte slice. Returns `None` for
    /// system/real-time messages and anything not bindable. A Note On with
    /// velocity 0 is the conventional Note Off and is reported as such.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let &status = bytes.first()?;
        // System/real-time (0xF0..) carry no channel and are not bindable.
        if status >= 0xF0 {
            return None;
        }
        let kind = status & 0xF0;
        let channel = (status & 0x0F) + 1;
        match kind {
            0x80 => {
                let note = *bytes.get(1)?;
                Some(Self::NoteOff { channel, note })
            }
            0x90 => {
                let note = *bytes.get(1)?;
                let velocity = *bytes.get(2)?;
                if velocity == 0 {
                    Some(Self::NoteOff { channel, note })
                } else {
                    Some(Self::NoteOn {
                        channel,
                        note,
                        velocity,
                    })
                }
            }
            0xB0 => {
                let controller = *bytes.get(1)?;
                let value = *bytes.get(2)?;
                Some(Self::ControlChange {
                    channel,
                    controller,
                    value,
                })
            }
            0xC0 => {
                let program = *bytes.get(1)?;
                Some(Self::ProgramChange { channel, program })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
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
}
