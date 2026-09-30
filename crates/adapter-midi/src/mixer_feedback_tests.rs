//! #1007 — the bytes a surface needs to show a mixer strip's state: motor
//! fader position and MUTE LED, derived from the same bindings that read it.
use super::*;

fn bindings(yaml: &str) -> Vec<Binding> {
    serde_yaml::from_str(yaml).unwrap()
}

const SURFACE: &str = r#"
- source: { kind: pitch_bend, channel: 2 }
  command: SetMixerFader
  args: { strip: "in:0@dev" }
  scale: { min: -60.0, max: 12.0, into: gain_db }
- source: { kind: note_on, channel: 1, note: 17 }
  command: ToggleMixerMute
  args: { strip: "in:0@dev" }
- source: { kind: cc, channel: 1, controller: 7 }
  command: SetMixerFader
  args: { strip: "out:0,1@dev" }
  scale: { min: -60.0, max: 12.0, into: gain_db }
- source: { kind: note_on, channel: 1, note: 18 }
  command: SetMixerMute
  args: { strip: "out:0,1@dev", muted: true }
- source: { kind: note_on, channel: 1, note: 60 }
  command: SaveProject
"#;

#[test]
fn pitch_bend_fader_moves_to_the_strip_gain() {
    let out = strip_feedback(&bindings(SURFACE), "in:0@dev", 12.0, false);
    // Top of the travel: 16383 → LSB 0x7F, MSB 0x7F on channel 2 (0xE1).
    assert!(out.contains(&[0xE1, 0x7F, 0x7F]), "got {out:02X?}");
}

#[test]
fn pitch_bend_fader_bottom_is_zero() {
    let out = strip_feedback(&bindings(SURFACE), "in:0@dev", -60.0, false);
    assert!(out.contains(&[0xE1, 0x00, 0x00]), "got {out:02X?}");
}

#[test]
fn mute_led_lights_when_muted() {
    let out = strip_feedback(&bindings(SURFACE), "in:0@dev", 0.0, true);
    assert!(out.contains(&[0x90, 17, 127]), "got {out:02X?}");
}

#[test]
fn mute_led_goes_dark_when_unmuted() {
    let out = strip_feedback(&bindings(SURFACE), "in:0@dev", 0.0, false);
    assert!(out.contains(&[0x90, 17, 0]), "got {out:02X?}");
}

#[test]
fn cc_fader_sends_seven_bit_position() {
    let out = strip_feedback(&bindings(SURFACE), "out:0,1@dev", 12.0, true);
    assert!(out.contains(&[0xB0, 7, 127]), "got {out:02X?}");
    assert!(out.contains(&[0x90, 18, 127]), "got {out:02X?}");
}

#[test]
fn other_strips_and_commands_send_nothing() {
    let out = strip_feedback(&bindings(SURFACE), "in:9@other", 0.0, true);
    assert!(out.is_empty(), "got {out:02X?}");
}

#[test]
fn only_the_strip_s_own_bindings_answer() {
    let out = strip_feedback(&bindings(SURFACE), "in:0@dev", 0.0, false);
    assert_eq!(out.len(), 2, "got {out:02X?}");
}
