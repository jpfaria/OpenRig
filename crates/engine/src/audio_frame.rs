//! Responsibility: carries one frame of audio in whichever layout the stream has.
//!
//! Split out of `runtime_audio_frame.rs` (#873). Lives on the audio thread:
//! `Copy`, no allocation, no branching beyond the layout match.

use block_core::AudioChannelLayout;

#[derive(Debug, Clone, Copy)]
pub(crate) enum AudioFrame {
    Mono(f32),
    Stereo([f32; 2]),
}

impl AudioFrame {
    #[inline(always)]
    pub(crate) fn mono_mix(self) -> f32 {
        match self {
            AudioFrame::Mono(sample) => sample,
            AudioFrame::Stereo([left, right]) => (left + right) * 0.5,
        }
    }

    /// Linear gain applied to the frame. Used to apply `Chain.volume`
    /// BEFORE the output limiter so the limiter (in `write_output_frame`)
    /// sees the post-volume signal and holds a hot chain ≤ full scale
    /// instead of clipping at the DAC (volume × already-limited signal).
    #[inline(always)]
    pub(crate) fn scaled(self, k: f32) -> AudioFrame {
        match self {
            AudioFrame::Mono(s) => AudioFrame::Mono(s * k),
            AudioFrame::Stereo([l, r]) => AudioFrame::Stereo([l * k, r * k]),
        }
    }
}

#[inline(always)]
pub(crate) fn read_input_frame(
    input_layout: AudioChannelLayout,
    input_channels: &[usize],
    frame: &[f32],
) -> AudioFrame {
    match input_layout {
        AudioChannelLayout::Mono => AudioFrame::Mono(read_channel(frame, input_channels[0])),
        AudioChannelLayout::Stereo => AudioFrame::Stereo([
            read_channel(frame, input_channels[0]),
            read_channel(frame, input_channels[1]),
        ]),
    }
}

#[inline(always)]
pub(crate) fn read_channel(frame: &[f32], channel_index: usize) -> f32 {
    frame.get(channel_index).copied().unwrap_or(0.0)
}

#[inline(always)]
pub(crate) fn silent_frame(layout: AudioChannelLayout) -> AudioFrame {
    match layout {
        AudioChannelLayout::Mono => AudioFrame::Mono(0.0),
        AudioChannelLayout::Stereo => AudioFrame::Stereo([0.0, 0.0]),
    }
}

/// Sum two audio frames together (for mixing multiple input streams).
#[allow(dead_code)]
pub(crate) fn mix_frames(a: AudioFrame, b: AudioFrame) -> AudioFrame {
    match (a, b) {
        (AudioFrame::Mono(l), AudioFrame::Mono(r)) => AudioFrame::Mono(l + r),
        (AudioFrame::Stereo([l1, r1]), AudioFrame::Stereo([l2, r2])) => {
            AudioFrame::Stereo([l1 + l2, r1 + r2])
        }
        (AudioFrame::Mono(m), AudioFrame::Stereo([l, r])) => AudioFrame::Stereo([m + l, m + r]),
        (AudioFrame::Stereo([l, r]), AudioFrame::Mono(m)) => AudioFrame::Stereo([l + m, r + m]),
    }
}
