//! Per-chain auto-max loudness (issue #402).
//!
//! Sits at the very end of every chain (after the user's last block,
//! before the stream tap and mixdown). Tracks the running output peak
//! with a fast-attack / slow-release envelope and applies a smoothed
//! gain so the chain output sits at `TARGET_PEAK_DBFS` regardless of
//! which amp / pedal / preamp the user dropped in.
//!
//! Boost-only: a chain that's already at or above target is left
//! alone — "as chains tem que ficar no volume maximo" — but anything
//! quieter is pushed up. Hard ceiling on the boost so a glitch at
//! near-silence can't apply +60 dB.
//!
//! Audio-thread safe: only `f32` arithmetic, no allocs / locks /
//! syscalls per sample.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::runtime_audio_frame::AudioFrame;

/// Process-global enable flag. Default OFF so the runtime preserves
/// volume invariant #10 for any caller that doesn't opt in (every
/// test path inherits this default).  Production binaries flip this
/// on in their `main` (see `adapter-gui` / `adapter-console`).
static RUNTIME_DEFAULT_ENABLED: AtomicBool = AtomicBool::new(false);

/// Production binaries call this during startup to enable the
/// per-chain auto-max for the rest of the process. No effect on
/// existing chains — only chains built AFTER this call get an
/// enabled `AutoMaxState`.
///
/// `OPENRIG_AUTO_MAX_OFF=1` env var overrides this back to OFF
/// (read once at flag-flip time) — useful for in-the-field A/B
/// testing without rebuilding.
pub fn set_runtime_default_enabled(enabled: bool) {
    let env_disable = std::env::var("OPENRIG_AUTO_MAX_OFF")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    RUNTIME_DEFAULT_ENABLED.store(enabled && !env_disable, Ordering::Relaxed);
}

#[inline]
fn current_default_enabled() -> bool {
    RUNTIME_DEFAULT_ENABLED.load(Ordering::Relaxed)
}

/// Snapshot of the process-global flag (after env-override). For
/// diagnostics and tests — don't gate audio paths on this.
pub fn is_runtime_default_enabled() -> bool {
    current_default_enabled()
}

/// Loudness target — drives perceived volume. Sits high enough that
/// naturally-loud saturated amp signals (already around -5 to -8
/// dBFS RMS) don't sit above the target while clean signals stay
/// 10+ dB below them. The user's complaint in #413 was exactly that
/// gap: clean chains (heartbreak warfare) feeling much quieter than
/// saturated ones (basket case). A higher target lifts the cleans
/// up to where the saturated already are.
pub const TARGET_RMS_DBFS: f32 = -3.0;

/// Soft-knee target for the follower's `allowed_by_peak` calculation.
/// Conservative — the boost stops growing once the peak follower
/// reaches this value, BUT individual samples can still ride above
/// it (see `HARD_CAP_DBFS` below) because the soft saturator
/// compresses them smoothly. Net effect: clean signals with high
/// crest factor reach the loudness target via transient saturation
/// without the gain follower itself running the chain hot.
pub const PEAK_CEILING_DBFS: f32 = -3.0;

/// Absolute hard cap on any output sample, after gain. Sits just
/// under 0 dBFS to give the OS DAC its minimum safety margin. The
/// soft saturator compresses samples between `PEAK_CEILING_DBFS` and
/// this cap with a tanh knee — no sample ever exceeds the cap.
pub const HARD_CAP_DBFS: f32 = -0.1;

/// Maximum boost the auto-max can apply, in dB.
pub const MAX_BOOST_DB: f32 = 24.0;

/// RMS follower window — how many samples of history feed the
/// running-RMS estimate. Long enough to track perceived loudness on
/// guitar (which has slow envelopes), short enough that the gain
/// settles within a second on a level change.
pub const RMS_WINDOW_MS: f32 = 300.0;

/// Peak follower attack — fast so the peak ceiling catches transients.
pub const PEAK_ATTACK_MS: f32 = 1.0;

/// Peak follower release — slow so the peak ceiling doesn't release
/// between transients and let bursts through.
pub const PEAK_RELEASE_MS: f32 = 250.0;

/// Gain smoothing time. Long enough to avoid zipper, short enough to
/// feel responsive when the user changes amps.
pub const GAIN_SMOOTH_MS: f32 = 200.0;

/// Below this RMS (linear) the chain is treated as silent and the
/// auto-max stops chasing — otherwise idle noise floor would be
/// boosted to the RMS target and explode at the next note.
const SILENCE_RMS_THRESHOLD_LIN: f32 = 1.0e-4; // ≈ -80 dBFS

pub struct AutoMaxState {
    /// Smoothed running mean-square (linear). RMS = sqrt(mean_square).
    mean_square: f32,
    /// Smoothed running peak (linear). Used as a guard so the boost
    /// doesn't push the chain output past the peak ceiling.
    peak_envelope: f32,
    /// Applied gain (linear). Smoothed toward `desired_gain`.
    current_gain: f32,
    /// Pre-computed coefficients (sample-rate dependent).
    rms_coeff: f32,
    peak_attack_coeff: f32,
    peak_release_coeff: f32,
    smooth_coeff: f32,
    target_rms_lin: f32,
    peak_ceiling_lin: f32,
    hard_cap_lin: f32,
    max_gain_lin: f32,
    /// Frozen at construction time — flipping the global default mid-run
    /// shouldn't change a chain that's already processing audio.
    enabled: bool,
}

impl AutoMaxState {
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self::with_enabled(sample_rate, current_default_enabled())
    }

    pub(crate) fn with_enabled(sample_rate: f32, enabled: bool) -> Self {
        Self {
            mean_square: 0.0,
            peak_envelope: 0.0,
            current_gain: 1.0,
            rms_coeff: time_to_coeff(RMS_WINDOW_MS, sample_rate),
            peak_attack_coeff: time_to_coeff(PEAK_ATTACK_MS, sample_rate),
            peak_release_coeff: time_to_coeff(PEAK_RELEASE_MS, sample_rate),
            smooth_coeff: time_to_coeff(GAIN_SMOOTH_MS, sample_rate),
            target_rms_lin: db_to_lin(TARGET_RMS_DBFS),
            peak_ceiling_lin: db_to_lin(PEAK_CEILING_DBFS),
            hard_cap_lin: db_to_lin(HARD_CAP_DBFS),
            max_gain_lin: db_to_lin(MAX_BOOST_DB),
            enabled,
        }
    }

    /// Apply auto-max to a buffer of frames in place. Audio-thread hot
    /// path — no allocs, branches kept tight. No-op when disabled.
    #[inline]
    pub(crate) fn process(&mut self, frames: &mut [AudioFrame]) {
        if !self.enabled {
            return;
        }
        for frame in frames.iter_mut() {
            self.process_frame(frame);
        }
        // Periodic info log (issue #413 diagnostic) — one line every
        // ~1 second of audio at 48 kHz / 256-frame callback (≈186
        // callbacks/s, log every 200). Allocates, so kept off the
        // per-sample path.
        static AUTO_MAX_LOG_COUNTER: std::sync::atomic::AtomicU32 =
            std::sync::atomic::AtomicU32::new(0);
        let n = AUTO_MAX_LOG_COUNTER
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n % 200 == 0 {
            let rms_db = if self.mean_square > 1e-12 {
                10.0 * self.mean_square.log10()
            } else {
                -120.0
            };
            let peak_db = if self.peak_envelope > 1e-6 {
                20.0 * self.peak_envelope.log10()
            } else {
                -120.0
            };
            let gain_db = if self.current_gain > 1e-6 {
                20.0 * self.current_gain.log10()
            } else {
                -120.0
            };
            log::info!(
                "auto_max: rms_in={:+.2}dB peak_in={:+.2}dB gain={:+.2}dB ({:.2}x)",
                rms_db,
                peak_db,
                gain_db,
                self.current_gain,
            );
        }
    }

    /// Diagnostics for integration tests — current state of the
    /// follower. Audio thread should never call this.
    #[doc(hidden)]
    pub fn snapshot(&self) -> (f32, f32, f32) {
        (self.mean_square.sqrt(), self.peak_envelope, self.current_gain)
    }

    /// Enabled flag readback for diagnostics.
    #[doc(hidden)]
    pub fn enabled_for_diag(&self) -> bool {
        self.enabled
    }

    #[inline(always)]
    fn process_frame(&mut self, frame: &mut AudioFrame) {
        let frame_peak = frame_abs_peak(frame);
        let frame_sq = frame_mean_square(frame);

        // RMS follower — single-pole, RMS_WINDOW_MS time constant.
        self.mean_square = self.rms_coeff * self.mean_square + (1.0 - self.rms_coeff) * frame_sq;

        // Peak follower — fast attack / slow release for the ceiling guard.
        if frame_peak > self.peak_envelope {
            self.peak_envelope = self.peak_attack_coeff * self.peak_envelope
                + (1.0 - self.peak_attack_coeff) * frame_peak;
        } else {
            self.peak_envelope = self.peak_release_coeff * self.peak_envelope
                + (1.0 - self.peak_release_coeff) * frame_peak;
        }

        // The follower has a 1 ms attack time-constant — perfect for
        // tracking sustained level but it lags during the first few
        // samples of a transient. Use `max(envelope, instantaneous)`
        // for the ceiling guard so the GAIN computed for THIS frame
        // already accounts for the transient that just landed.
        let guard_peak = self.peak_envelope.max(frame_peak);

        // Compute the boost from RMS, then guard with peak ceiling.
        let rms = self.mean_square.sqrt();
        let desired_gain = if rms < SILENCE_RMS_THRESHOLD_LIN {
            self.current_gain
        } else {
            let want_for_loudness = self.target_rms_lin / rms;
            // `allowed_by_peak` uses the HARD CAP, not the knee — the
            // soft saturator catches everything between `peak_ceiling`
            // and `hard_cap`, so the gain is free to push the running
            // peak right up to the cap. Using the knee here would
            // stall the boost too early and leave clean signal short
            // of the loudness target.
            let allowed_by_peak = if guard_peak > 1.0e-9 {
                self.hard_cap_lin / guard_peak
            } else {
                self.max_gain_lin
            };
            want_for_loudness
                .min(allowed_by_peak)
                .min(self.max_gain_lin)
                .max(1.0)
        };

        // Smooth the applied gain toward the desired gain.
        self.current_gain =
            self.smooth_coeff * self.current_gain + (1.0 - self.smooth_coeff) * desired_gain;

        // Hard ceiling AFTER gain — the smoothing inevitably lags step
        // changes ("first note after silence"), so without this clamp
        // the very first transient samples of a chord ride at the
        // pre-attack `current_gain` and overshoot the ceiling. Hard
        // clamp guarantees that NO sample ever exceeds the ceiling
        // regardless of follower / smoother dynamics. Distortion
        // produced by the clamp is bounded to the brief overshoot
        // window (a few ms at most) and is the source of the chiado
        // reported in #413.
        apply_gain_with_ceiling(
            frame,
            self.current_gain,
            self.peak_ceiling_lin,
            self.hard_cap_lin,
        );
    }
}

#[inline(always)]
fn frame_abs_peak(frame: &AudioFrame) -> f32 {
    match frame {
        AudioFrame::Mono(s) => s.abs(),
        AudioFrame::Stereo([l, r]) => l.abs().max(r.abs()),
    }
}

#[inline(always)]
fn frame_mean_square(frame: &AudioFrame) -> f32 {
    match frame {
        AudioFrame::Mono(s) => s * s,
        AudioFrame::Stereo([l, r]) => 0.5 * (l * l + r * r),
    }
}

#[inline(always)]
fn apply_gain_with_ceiling(frame: &mut AudioFrame, g: f32, knee: f32, hard_cap: f32) {
    match frame {
        AudioFrame::Mono(s) => *s = soft_saturate(*s * g, knee, hard_cap),
        AudioFrame::Stereo([l, r]) => {
            *l = soft_saturate(*l * g, knee, hard_cap);
            *r = soft_saturate(*r * g, knee, hard_cap);
        }
    }
}

/// Soft-knee saturator — linear up to `knee`, then a tanh approach
/// to `hard_cap`. Output never exceeds `hard_cap` in absolute value.
///
/// Why two thresholds (#413): the follower's `allowed_by_peak` uses
/// `knee` as the soft target — gain stops growing when the follower
/// reaches it. But the gain is smoothed in time, so individual
/// transient samples can land well above `knee` for short bursts.
/// Letting them ride up to `hard_cap` (just under 0 dBFS) under a
/// tanh curve preserves the loudness target's RMS for clean signals
/// (which have peak ≫ RMS) without ever clipping the OS DAC.
#[inline(always)]
fn soft_saturate(x: f32, knee: f32, hard_cap: f32) -> f32 {
    let abs = x.abs();
    if abs <= knee {
        x
    } else {
        let sign = x.signum();
        let range = hard_cap - knee;
        let over = abs - knee;
        sign * (knee + range * (over / range).tanh())
    }
}

/// Convert a time constant in milliseconds into the IIR coefficient
/// `exp(-1 / (tau_seconds * sample_rate))`. Higher = slower follower.
#[inline]
fn time_to_coeff(time_ms: f32, sample_rate: f32) -> f32 {
    let tau_samples = (time_ms / 1000.0) * sample_rate;
    if tau_samples <= 0.0 {
        0.0
    } else {
        (-1.0 / tau_samples).exp()
    }
}

#[inline]
fn db_to_lin(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

#[cfg(test)]
#[path = "auto_max_tests.rs"]
mod tests;
