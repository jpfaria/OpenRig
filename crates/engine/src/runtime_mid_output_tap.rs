//! Mid-chain output taps (issue #85).
//!
//! An `Output` block placed BETWEEN effect blocks emits the signal as
//! processed up to its own position while the chain keeps flowing to the
//! blocks after it — a non-destructive tap, not a cut. `process_single_segment`
//! calls [`emit_mid_output_taps`] as it walks the segment's blocks.
//!
//! Audio-thread hot path: no allocation beyond the route buffer's first
//! callback growth, no locking, no syscalls — same contract as the rest of
//! the callback.

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_segments::SegmentTap;
use crate::runtime_state::{InputCallbackScratch, FADE_IN_FRAMES};

/// Gain of the click-safe rebuild fade at frame `i` of this callback, given
/// the counter the segment entered the callback with. Pure mirror of the
/// in-place ramp `process_single_segment` applies to the segment's own
/// buffer, so every route of a rebuilt segment ramps identically.
#[inline]
fn fade_in_gain(fade_at_entry: usize, i: usize) -> f32 {
    if i >= fade_at_entry {
        return 1.0;
    }
    let progress = 1.0 - ((fade_at_entry - i) as f32 / FADE_IN_FRAMES as f32);
    0.5 * (1.0 - (std::f32::consts::PI * progress).cos())
}

#[inline]
fn add_frames(acc: AudioFrame, add: AudioFrame) -> AudioFrame {
    match (acc, add) {
        (AudioFrame::Stereo([l1, r1]), AudioFrame::Stereo([l2, r2])) => {
            AudioFrame::Stereo([l1 + l2, r1 + r2])
        }
        (AudioFrame::Mono(a), AudioFrame::Mono(b)) => AudioFrame::Mono(a + b),
        (AudioFrame::Stereo([l, r]), AudioFrame::Mono(m)) => AudioFrame::Stereo([l + m, r + m]),
        (AudioFrame::Mono(m), AudioFrame::Stereo([l, r])) => AudioFrame::Stereo([m + l, m + r]),
    }
}

/// Publish the segment's current bus to every mid-chain `Output` sitting
/// exactly after `processed` of the segment's blocks.
#[inline]
pub(crate) fn emit_mid_output_taps(
    taps: &[SegmentTap],
    processed: usize,
    frame_buffer: &[AudioFrame],
    fade_at_entry: usize,
    scratch: &mut InputCallbackScratch,
) {
    if taps.is_empty() {
        return;
    }
    for tap in taps.iter().filter(|t| t.blocks_before == processed) {
        let buf = scratch.mixed_per_route.entry(tap.route_idx).or_default();
        for (i, &frame) in frame_buffer.iter().enumerate() {
            let gain = fade_in_gain(fade_at_entry, i);
            let faded = match frame {
                AudioFrame::Mono(s) => AudioFrame::Mono(s * gain),
                AudioFrame::Stereo([l, r]) => AudioFrame::Stereo([l * gain, r * gain]),
            };
            if i < buf.len() {
                buf[i] = add_frames(buf[i], faded);
            } else {
                buf.push(faded);
            }
        }
    }
}
