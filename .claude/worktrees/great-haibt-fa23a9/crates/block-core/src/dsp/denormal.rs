//! Denormal-number protection.
//!
//! When float values fall below ~1e-38 the CPU enters a slow gradual-
//! underflow path (subnormals) — typically 50–100x slower per op,
//! enough to cause xruns in feedback paths after long silences.
//!
//! The portable fix is to inject a tiny constant into the recursive
//! state at every sample so the value never reaches the subnormal
//! range. Cost: one fadd per call. No ifdef, no platform-specific
//! intrinsic. Works on every cargo target.
//!
//! Use [`flush_denormal`] in feedback registers (delay-line read,
//! biquad state, reverb tap, comb feedback). It is a no-op for normal
//! audio levels and silently saves CPU when input goes silent.

/// Anti-denormal DC offset injected into recursive state. Chosen as
/// 1e-30: well above the f32 subnormal threshold (~1.18e-38) and well
/// below the noise floor of any practical audio path (~96 dB ≈ 1.5e-5).
pub const DENORMAL_GUARD: f32 = 1.0e-30;

/// Returns `x + DENORMAL_GUARD`. Inline so the compiler folds it into
/// the surrounding expression with zero overhead.
#[inline]
pub fn flush_denormal(x: f32) -> f32 {
    x + DENORMAL_GUARD
}

#[cfg(test)]
#[path = "denormal_tests.rs"]
mod tests;
