//! Audio thread I/O helpers (slice 5 of Phase 2 issue #194).
//!
//! Hot-path utilities: every function in this module runs per audio
//! callback. Lifted out of `runtime.rs` so the parent file gets closer
//! to the < 600 LOC cap.
//!
//! `#[inline]` attributes are PREEMPTIVE per Phase 2 slice 1 lesson:
//! when hot-path helpers cross a module boundary the compiler can stop
//! inlining them, blowing the buffer budget on the user's machine. We
//! force inlining at extraction time, not as a remediation step.
//!
//! What's here:
//!   - `ensure_flush_to_zero` (FZ bit on aarch64; no-op elsewhere)
//!   - `blend_frame` (dry/wet crossfade per audio frame)
//!   - `output_limiter` (tanh soft clipper above 0.95)
//!   - `write_output_frame` (apply per-stream mixdown to interleaved buf)
//!   - `apply_mixdown` (Stereo → Mono channel reduction modes)
//!   - `layout_from_channels` (channel count → AudioChannelLayout)
//!   - `layout_label` (diagnostics)
//!   - `downcast_panic_message` (catch_unwind payload formatter)

use std::any::Any;

use anyhow::{anyhow, Result};
use block_core::AudioChannelLayout;
use project::chain::ChainOutputMixdown;

use crate::runtime_audio_frame::AudioFrame;

/// Ensure denormalized floats are flushed to zero on aarch64.
///
/// Without this, neural-network processors (NAM) produce degraded audio on
/// aarch64 because denormals accumulate through the network layers.  On x86
/// the NAM/Eigen libraries set DAZ+FTZ internally — we leave x86 alone to
/// avoid changing the sound character on macOS/Windows.
#[inline(always)]
pub(crate) fn ensure_flush_to_zero() {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        // FZ bit (bit 24) in FPCR
        let fpcr: u64;
        core::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
        if fpcr & (1 << 24) == 0 {
            core::arch::asm!("msr fpcr, {}", in(reg) fpcr | (1 << 24));
        }
    }
}

pub(crate) fn downcast_panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

#[inline]
pub(crate) fn blend_frame(frame: &mut AudioFrame, dry: AudioFrame, dry_gain: f32, wet_gain: f32) {
    match (frame, dry) {
        (AudioFrame::Mono(w), AudioFrame::Mono(d)) => {
            *w = d * dry_gain + *w * wet_gain;
        }
        (AudioFrame::Stereo([wl, wr]), AudioFrame::Stereo([dl, dr])) => {
            *wl = dl * dry_gain + *wl * wet_gain;
            *wr = dr * dry_gain + *wr * wet_gain;
        }
        // Layout mismatch shouldn't happen in practice; pass dry through
        (frame, dry) => {
            *frame = dry;
        }
    }
}

#[inline]
pub(crate) fn output_limiter(sample: f32) -> f32 {
    if sample.abs() < 0.95 {
        sample
    } else {
        sample.tanh()
    }
}

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

#[inline]
pub(crate) fn apply_mixdown(mixdown: ChainOutputMixdown, left: f32, right: f32) -> f32 {
    match mixdown {
        ChainOutputMixdown::Sum => left + right,
        ChainOutputMixdown::Average => (left + right) * 0.5,
        ChainOutputMixdown::Left => left,
        ChainOutputMixdown::Right => right,
    }
}

#[allow(dead_code)]
pub(crate) fn layout_from_channels(channel_count: usize) -> Result<AudioChannelLayout> {
    match channel_count {
        1 => Ok(AudioChannelLayout::Mono),
        2 => Ok(AudioChannelLayout::Stereo),
        other => Err(anyhow!(
            "only mono and stereo are supported right now; got {} channels",
            other
        )),
    }
}

pub(crate) fn layout_label(layout: AudioChannelLayout) -> &'static str {
    match layout {
        AudioChannelLayout::Mono => "mono",
        AudioChannelLayout::Stereo => "stereo",
    }
}
