//! Audio thread output I/O — writing the per-stream `AudioFrame` into
//! the interleaved `f32` buffer the audio backend hands us.
//!
//! Single-responsibility: this module exists to take a processed
//! `AudioFrame` (the runtime's internal format) and place its samples
//! into the right slots of the device's interleaved output buffer,
//! applying the route's mixdown / limiter on the way out.
//!
//! Hot-path: every audio callback. Marked `#[inline]` preemptively
//! per Phase 2 slice 1 lesson — same-module inlining heuristics break
//! across module boundaries; we force inlining at extraction time so
//! the compiler keeps generating the same code shape it did before
//! the move.
//!
//! What's NOT here:
//!   - DSP math (limiter, mixdown, blend) → `runtime_dsp.rs`
//!   - Channel-layout type helpers → `runtime_layout.rs`
//!   - Reading the input buffer → `runtime_audio_frame::read_input_frame`

use project::chain::ChainOutputMixdown;

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_dsp::{apply_mixdown, output_limiter};

#[inline]
pub(crate) fn write_output_frame(
    chain_frame: AudioFrame,
    output_channels: &[usize],
    frame: &mut [f32],
    mixdown: ChainOutputMixdown,
) {
    match chain_frame {
        AudioFrame::Mono(sample) => {
            let limited = output_limiter(sample);
            for &channel_index in output_channels {
                if let Some(dst) = frame.get_mut(channel_index) {
                    *dst = limited;
                }
            }
        }
        AudioFrame::Stereo([left, right]) => match output_channels {
            [] => {}
            [channel_index] => {
                if let Some(dst) = frame.get_mut(*channel_index) {
                    *dst = output_limiter(apply_mixdown(mixdown, left, right));
                }
            }
            [left_channel, right_channel, ..] => {
                if let Some(dst) = frame.get_mut(*left_channel) {
                    *dst = output_limiter(left);
                }
                if let Some(dst) = frame.get_mut(*right_channel) {
                    *dst = output_limiter(right);
                }
            }
        },
    }
}
