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
pub fn set_runtime_default_enabled(enabled: bool) {
    RUNTIME_DEFAULT_ENABLED.store(enabled, Ordering::Relaxed);
}

#[inline]
fn current_default_enabled() -> bool {
    RUNTIME_DEFAULT_ENABLED.load(Ordering::Relaxed)
}

/// Where every chain's running peak should land.
pub const TARGET_PEAK_DBFS: f32 = -1.0;

/// Maximum boost the auto-max can apply, in dB.
pub const MAX_BOOST_DB: f32 = 24.0;

/// Envelope follower attack — how fast the running-peak follower
/// reacts to a louder sample. Short so transients are caught.
pub const ATTACK_MS: f32 = 5.0;

/// Envelope follower release — how fast the running-peak follower
/// decays back when the signal gets quieter. Slow so the gain
/// doesn't pump on note decays.
pub const RELEASE_MS: f32 = 500.0;

/// Gain smoothing time — how fast the applied gain interpolates
/// toward the desired gain. Slow enough to avoid zipper noise on
/// step changes, fast enough to feel responsive when the user
/// changes captures.
pub const GAIN_SMOOTH_MS: f32 = 100.0;

/// Below this peak (linear) the chain is treated as silent and the
/// auto-max stops chasing — otherwise idle noise floor would get
/// boosted to 0 dB and explode at the next note.
const SILENCE_THRESHOLD_LIN: f32 = 1.0e-4; // ≈ -80 dBFS

pub(crate) struct AutoMaxState {
    /// Smoothed running peak (linear, |sample|).
    peak_envelope: f32,
    /// Applied gain (linear). Smoothed toward `desired_gain`.
    current_gain: f32,
    /// Pre-computed coefficients (sample-rate dependent).
    attack_coeff: f32,
    release_coeff: f32,
    smooth_coeff: f32,
    target_peak_lin: f32,
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
            peak_envelope: 0.0,
            current_gain: 1.0,
            attack_coeff: time_to_coeff(ATTACK_MS, sample_rate),
            release_coeff: time_to_coeff(RELEASE_MS, sample_rate),
            smooth_coeff: time_to_coeff(GAIN_SMOOTH_MS, sample_rate),
            target_peak_lin: db_to_lin(TARGET_PEAK_DBFS),
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
    }

    #[inline(always)]
    fn process_frame(&mut self, frame: &mut AudioFrame) {
        let frame_peak = frame_abs_peak(frame);
        // Envelope follower (fast attack, slow release).
        if frame_peak > self.peak_envelope {
            self.peak_envelope =
                self.attack_coeff * self.peak_envelope + (1.0 - self.attack_coeff) * frame_peak;
        } else {
            self.peak_envelope =
                self.release_coeff * self.peak_envelope + (1.0 - self.release_coeff) * frame_peak;
        }

        // Desired gain — boost-only, capped, and frozen on silence.
        let desired_gain = if self.peak_envelope < SILENCE_THRESHOLD_LIN {
            self.current_gain
        } else {
            (self.target_peak_lin / self.peak_envelope)
                .min(self.max_gain_lin)
                .max(1.0)
        };

        // Smooth the applied gain toward the desired gain.
        self.current_gain =
            self.smooth_coeff * self.current_gain + (1.0 - self.smooth_coeff) * desired_gain;

        apply_gain(frame, self.current_gain);
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
fn apply_gain(frame: &mut AudioFrame, g: f32) {
    match frame {
        AudioFrame::Mono(s) => *s *= g,
        AudioFrame::Stereo([l, r]) => {
            *l *= g;
            *r *= g;
        }
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
