//! Audio thread DSP + per-callback utilities.
//!
//! Hot-path math + setup that runs every audio callback. Lifted out of
//! `runtime.rs` so the parent file gets closer to the < 600 LOC cap.
//! Per Phase 2 slice 1 lesson, hot-path helpers crossing a module
//! boundary are marked `#[inline]` (or `#[inline(always)]` for the
//! tiniest ones) at extraction time so rustc keeps inlining them.
//!
//! What's here:
//!   - `ensure_flush_to_zero` — sets FZ bit on aarch64 FPCR so NAM
//!     network output doesn't degrade through accumulating denormals.
//!     No-op on x86 (NAM/Eigen handle DAZ+FTZ internally there).
//!   - `blend_frame` — dry/wet crossfade for Insert send/return blend.
//!   - `output_limiter` — tanh soft clipper above 0.95 — the chain's
//!     last line of defence against samples clipping over ±1.0.
//!   - `apply_mixdown` — Stereo → Mono channel reduction modes (Sum /
//!     Average / Left / Right). Used by `write_output_frame` when an
//!     output route is mono.
//!   - `downcast_panic_message` — pulls the `&str` / `String` payload
//!     out of `catch_unwind` so a faulted DSP block can be reported
//!     to the UI instead of taking down the audio thread.
//!
//! What's NOT here: actual buffer I/O (writing to the interleaved
//! output buffer) lives in `runtime_io.rs`. Channel-layout type
//! helpers live in `runtime_layout.rs`.

use std::any::Any;

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

/// Soft limiter — transparent below 0dBFS, gentle saturation above.
#[inline]
pub(crate) fn output_limiter(sample: f32) -> f32 {
    if sample.abs() < 0.95 {
        sample
    } else {
        sample.tanh()
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
