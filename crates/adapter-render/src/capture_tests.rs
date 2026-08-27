//! #913 — how a device's interleaved capture becomes stereo frames.
//!
//! CLAUDE.md invariant 5: a stream is stereo internally, so a mono interface
//! must be BROADCAST to both sides, never left silent on one. A multi-channel
//! interface contributes its first two channels. A partial trailing frame is
//! dropped rather than padded — half a frame would be a click at the tail.

use super::interleaved_to_stereo;

#[test]
fn a_mono_capture_is_broadcast_to_both_sides() {
    let frames = interleaved_to_stereo(&[0.1, -0.2, 0.3], 1);
    assert_eq!(frames, vec![[0.1, 0.1], [-0.2, -0.2], [0.3, 0.3]]);
}

#[test]
fn a_stereo_capture_keeps_the_two_sides_independent() {
    let frames = interleaved_to_stereo(&[0.1, 0.2, 0.3, 0.4], 2);
    assert_eq!(frames, vec![[0.1, 0.2], [0.3, 0.4]]);
}

#[test]
fn a_multichannel_capture_takes_the_first_two_channels() {
    // 4-channel interface: frame = [ch0, ch1, ch2, ch3].
    let frames = interleaved_to_stereo(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], 4);
    assert_eq!(frames, vec![[1.0, 2.0], [5.0, 6.0]]);
}

#[test]
fn a_partial_trailing_frame_is_dropped_not_padded() {
    let frames = interleaved_to_stereo(&[0.1, 0.2, 0.3], 2);
    assert_eq!(frames, vec![[0.1, 0.2]], "half a frame would click");
}

#[test]
fn an_empty_capture_yields_no_frames() {
    assert!(interleaved_to_stereo(&[], 2).is_empty());
    assert!(interleaved_to_stereo(&[], 1).is_empty());
}

#[test]
fn a_zero_channel_config_yields_no_frames_instead_of_dividing_by_zero() {
    assert!(interleaved_to_stereo(&[0.1, 0.2], 0).is_empty());
}
