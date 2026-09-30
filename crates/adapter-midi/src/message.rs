//! Responsibility: parses raw MIDI bytes into a channel message.
//! Parse raw MIDI bytes into the small set of channel-voice messages a
//! controller binding can target. Pure and fully testable — no device, no
//! `midir`. System/real-time/unsupported messages parse to `None`.

/// Top of a 14-bit Pitch Bend value.
pub const PITCH_BEND_MAX: u16 = 0x3FFF;

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
    /// 14-bit position, `0..=16383`.
    PitchBend {
        channel: u8,
        value: u16,
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
            0xE0 => {
                // 14-bit value, LSB first.
                let lsb = u16::from(*bytes.get(1)? & 0x7F);
                let msb = u16::from(*bytes.get(2)? & 0x7F);
                Some(Self::PitchBend {
                    channel,
                    value: (msb << 7) | lsb,
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
