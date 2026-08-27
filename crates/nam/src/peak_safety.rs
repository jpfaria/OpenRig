//! Responsibility: keeps the model's output from clipping the stream.

/// Memoryless output saturator (issue #496).
///
/// A loud loudness calibration must not be allowed to clip on the
/// converter (harsh digital distortion) or amplify the model noise
/// floor into a hard wall on the decay. This rounds only the peaks
/// that would exceed full-scale: transparent below `THRESHOLD` (a
/// normally-played, well-calibrated model never reaches it, so tone
/// and loudness are untouched), then smoothly asymptotic to ±1.0 —
/// musical saturation instead of a ±1.0 brickwall. Memoryless: zero
/// latency, zero state, deterministic, safe on the audio thread.
/// Knee above which the peak safety starts to saturate. Single source of
/// truth shared by [`soft_clip`] and its antiderivative [`soft_clip_int`].
const SOFT_CLIP_THRESHOLD: f32 = 0.8;

#[inline]
pub(crate) fn soft_clip(x: f32) -> f32 {
    const T: f32 = SOFT_CLIP_THRESHOLD;
    let a = x.abs();
    if a <= T {
        x
    } else {
        let over = a - T;
        x.signum() * (T + (1.0 - T) * (over / ((1.0 - T) + over)))
    }
}

/// Antiderivative `F` of [`soft_clip`] (`F'(x) == soft_clip(x)`), used for
/// first-order ADAA. `soft_clip` is odd, so `F` is even and continuous at
/// the knee (both branches meet at `T²/2`). Below the knee `soft_clip` is
/// the identity, so `F(x) = x²/2`; above it the closed form integrates
/// `1 - b²/(|x| - 2T + 1)` with `b = 1 - T`, offset by `K` for continuity.
#[inline]
pub(crate) fn soft_clip_int(x: f32) -> f32 {
    const T: f32 = SOFT_CLIP_THRESHOLD;
    let a = x.abs();
    if a <= T {
        0.5 * x * x
    } else {
        let b = 1.0 - T;
        let k = T - b * b * b.ln() - 0.5 * T * T;
        a - b * b * (a - 2.0 * T + 1.0).ln() - k
    }
}

/// Peak-safety saturation stage (issue #496/#675).
///
/// `soft_clip` is a memoryless nonlinearity. Applied per-sample at the base
/// sample rate it generates harmonics above Nyquist that fold back into the
/// audible band as inharmonic aliasing — heard as harsh hiss ("xiado"),
/// continuous whenever a loud-calibrated capture parks its output past the
/// knee. Invariant #2 (no added aliasing) forbids that.
///
/// This stage carries one sample of state so it can anti-alias the
/// saturation with first-order antiderivative anti-aliasing (ADAA, Parker
/// et al. 2016) — zero added latency, no oversampling, deterministic, safe
/// on the audio thread (invariants #1/#7/#8 all untouched).
pub(crate) struct PeakSafety {
    prev_in: f32,
}

impl PeakSafety {
    pub(crate) fn new() -> Self {
        Self { prev_in: 0.0 }
    }

    /// Saturate one sample with first-order ADAA, carrying state forward.
    ///
    /// `y = (F(x) - F(x₋₁)) / (x - x₋₁)` is the average of `soft_clip` over
    /// `[x₋₁, x]` — it band-limits the saturation, collapsing the aliasing
    /// the per-sample form folds into the audible band. When successive
    /// inputs are nearly equal the divided difference is ill-conditioned
    /// (0/0), so fall back to direct evaluation at the midpoint. The half-
    /// sample group delay (~10 µs at 48 kHz) is far below the latency
    /// budget (invariant #1).
    #[inline]
    pub(crate) fn process_one(&mut self, x: f32) -> f32 {
        const T: f32 = SOFT_CLIP_THRESHOLD;
        const ILL_CONDITIONED: f32 = 1.0e-5;
        let x0 = self.prev_in;
        self.prev_in = x;
        // Both samples below the knee: soft_clip is the identity over the
        // whole span, so there is nothing nonlinear to anti-alias. Stay
        // byte-exact transparent — averaging here would low-pass clean
        // signal and dull the tone (#413/#496). Only the saturating span
        // pays the ADAA divided difference.
        if x.abs() <= T && x0.abs() <= T {
            return x;
        }
        if (x - x0).abs() < ILL_CONDITIONED {
            return soft_clip(0.5 * (x + x0));
        }
        (soft_clip_int(x) - soft_clip_int(x0)) / (x - x0)
    }

    /// Saturate a whole buffer in place.
    pub(crate) fn process_block(&mut self, buffer: &mut [f32]) {
        for sample in buffer.iter_mut() {
            *sample = self.process_one(*sample);
        }
    }
}

// -----------------------------------------------------------------------
// Offline diagnostics API (issue #623 req #2).
//
// `open_model_diag` / `nam_process` / `close_model_diag` are the stable,
// public, slice-based entry points used by offline tooling (the
// OpenRig-plugins catalog audit / LUFS measurement gate) to push audio
// through a NAM model OUTSIDE the realtime `NamProcessor`. They wrap the
// same `cpp/nam_wrapper` FFI the runtime uses, so they exercise the
// identical signal chain. NEVER call these on the audio thread — they
// allocate (model load) and are offline-only.
//
// The #612 FFI rewrite (commit a9874a18) dropped these helpers and left
// only the private `nam_process` extern, which broke the OpenRig-plugins
// gate (E0603/E0432). They are restored here over the current wrapper FFI.
// -----------------------------------------------------------------------
