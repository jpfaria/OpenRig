//! DSP utility primitives used by every block processor crate —
//! gain conversion, envelope follower, one-pole filters, biquad.
//!
//! Lifted out of `lib.rs` (Phase 6 of issue #194). One responsibility:
//! tiny self-contained DSP building blocks that the per-effect crates
//! compose. No I/O, no logging, no allocation in the hot path.

use std::f32::consts::PI;

/// Capitalize the first character of a string, leaving the rest unchanged.
pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut result = String::with_capacity(s.len());
            for c in first.to_uppercase() {
                result.push(c);
            }
            result.push_str(chars.as_str());
            result
        }
    }
}

pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn lin_to_db(lin: f32) -> f32 {
    if lin > 1e-10 {
        20.0 * lin.log10()
    } else {
        -200.0
    }
}

pub fn calculate_coefficient(time_ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (sample_rate * 0.001 * time_ms.max(0.001))).exp()
}

pub struct EnvelopeFollower {
    envelope: f32,
    attack_coeff: f32,
    release_coeff: f32,
}

impl EnvelopeFollower {
    pub fn from_ms(attack_ms: f32, release_ms: f32, sample_rate: f32) -> Self {
        Self {
            envelope: 0.0,
            attack_coeff: calculate_coefficient(attack_ms, sample_rate),
            release_coeff: calculate_coefficient(release_ms, sample_rate),
        }
    }

    pub fn set_attack_coeff(&mut self, coeff: f32) {
        self.attack_coeff = coeff;
    }

    pub fn set_release_coeff(&mut self, coeff: f32) {
        self.release_coeff = coeff;
    }

    pub fn value(&self) -> f32 {
        self.envelope
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let abs_input = input.abs();
        if abs_input > self.envelope {
            self.envelope = self
                .attack_coeff
                .mul_add(self.envelope, (1.0 - self.attack_coeff) * abs_input);
        } else {
            self.envelope = self
                .release_coeff
                .mul_add(self.envelope, (1.0 - self.release_coeff) * abs_input);
        }
        self.envelope
    }
}

pub struct OnePoleLowPass {
    state: f32,
    coeff: f32,
}

impl OnePoleLowPass {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let coeff = 1.0 - (-2.0 * PI * cutoff_hz.max(1.0) / sample_rate).exp();
        Self { state: 0.0, coeff }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.state = self.coeff.mul_add(input - self.state, self.state);
        self.state
    }
}

pub struct OnePoleHighPass {
    prev_input: f32,
    prev_output: f32,
    coeff: f32,
}

impl OnePoleHighPass {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let rc = 1.0 / (2.0 * PI * cutoff_hz.max(1.0));
        let dt = 1.0 / sample_rate;
        let coeff = rc / (rc + dt);
        Self {
            prev_input: 0.0,
            prev_output: 0.0,
            coeff,
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.coeff * (self.prev_output + input - self.prev_input);
        self.prev_input = input;
        self.prev_output = output;
        output
    }
}

/// Second-order IIR (biquad) filter supporting peaking EQ, low shelf and high shelf.
///
/// Carries a target set of coefficients alongside the live ones so callers can
/// retune frequency / gain / Q without producing a click. While `ramp_remaining > 0`
/// each `process` call moves the live coefficients one fraction-of-a-step closer to
/// the target before computing the sample. State (`x1..y2`) is preserved across
/// retuning, so the filter never re-starts from a dead history.
pub struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    target_b0: f32,
    target_b1: f32,
    target_b2: f32,
    target_a1: f32,
    target_a2: f32,
    ramp_remaining: u32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

#[derive(Clone, Copy)]
pub enum BiquadKind {
    Peak,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
    Notch,
}

/// Number of samples over which `update_coefficients` ramps from the current
/// coefficients to the target. ~5.3 ms @ 48 kHz, which is short enough to feel
/// instant at the slider but long enough to suppress the parameter-change click
/// observed in issue #358.
pub const BIQUAD_COEFF_RAMP_FRAMES: u32 = 256;

/// 5-tuple of normalized biquad coefficients (b0, b1, b2, a1, a2) — the live and
/// target shapes both come from `compute_normalized_coefficients`.
struct NormalizedCoefficients {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

fn compute_normalized_coefficients(
    kind: BiquadKind,
    freq_hz: f32,
    gain_db: f32,
    q: f32,
    sample_rate: f32,
) -> NormalizedCoefficients {
    let w0 = 2.0 * PI * freq_hz / sample_rate;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();

    let (b0, b1, b2, a0, a1, a2) = match kind {
        BiquadKind::Peak => {
            let a = 10.0_f32.powf(gain_db / 40.0);
            let alpha = sin_w0 / (2.0 * q.max(0.01));
            (
                1.0 + alpha * a,
                -2.0 * cos_w0,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos_w0,
                1.0 - alpha / a,
            )
        }
        BiquadKind::LowShelf => {
            let a = 10.0_f32.powf(gain_db / 40.0);
            let alpha = sin_w0 / (2.0 * q.max(0.01));
            let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
            )
        }
        BiquadKind::HighShelf => {
            let a = 10.0_f32.powf(gain_db / 40.0);
            let alpha = sin_w0 / (2.0 * q.max(0.01));
            let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
            )
        }
        BiquadKind::HighPass => {
            let alpha = sin_w0 / (2.0 * q.max(0.01));
            (
                (1.0 + cos_w0) / 2.0,
                -(1.0 + cos_w0),
                (1.0 + cos_w0) / 2.0,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            )
        }
        BiquadKind::LowPass => {
            let alpha = sin_w0 / (2.0 * q.max(0.01));
            (
                (1.0 - cos_w0) / 2.0,
                1.0 - cos_w0,
                (1.0 - cos_w0) / 2.0,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            )
        }
        BiquadKind::Notch => {
            let alpha = sin_w0 / (2.0 * q.max(0.01));
            (
                1.0,
                -2.0 * cos_w0,
                1.0,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            )
        }
    };

    let inv_a0 = 1.0 / a0;
    NormalizedCoefficients {
        b0: b0 * inv_a0,
        b1: b1 * inv_a0,
        b2: b2 * inv_a0,
        a1: a1 * inv_a0,
        a2: a2 * inv_a0,
    }
}

impl BiquadFilter {
    pub fn new(kind: BiquadKind, freq_hz: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let c = compute_normalized_coefficients(kind, freq_hz, gain_db, q, sample_rate);
        Self {
            b0: c.b0,
            b1: c.b1,
            b2: c.b2,
            a1: c.a1,
            a2: c.a2,
            target_b0: c.b0,
            target_b1: c.b1,
            target_b2: c.b2,
            target_a1: c.a1,
            target_a2: c.a2,
            ramp_remaining: 0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Retune the filter live, preserving the IIR state (`x1..y2`).
    ///
    /// Stores the new normalized coefficients as the target. `process` will then
    /// linearly interpolate the live coefficients toward this target over
    /// [`BIQUAD_COEFF_RAMP_FRAMES`] samples. No allocation, no syscall — safe to
    /// invoke when the rebuild thread has exclusive ownership of the processor
    /// (issue #358 — runtime swap path in `try_reuse_block_node`).
    pub fn update_coefficients(
        &mut self,
        kind: BiquadKind,
        freq_hz: f32,
        gain_db: f32,
        q: f32,
        sample_rate: f32,
    ) {
        let c = compute_normalized_coefficients(kind, freq_hz, gain_db, q, sample_rate);
        self.target_b0 = c.b0;
        self.target_b1 = c.b1;
        self.target_b2 = c.b2;
        self.target_a1 = c.a1;
        self.target_a2 = c.a2;
        self.ramp_remaining = BIQUAD_COEFF_RAMP_FRAMES;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        // Coefficient smoothing — when ramp_remaining > 0, advance one step
        // toward target before computing the sample. Linear ramp over
        // `BIQUAD_COEFF_RAMP_FRAMES`. Keeps the per-sample cost to a handful
        // of f32 mul/add.
        if self.ramp_remaining > 0 {
            let inv = 1.0 / self.ramp_remaining as f32;
            self.b0 += (self.target_b0 - self.b0) * inv;
            self.b1 += (self.target_b1 - self.b1) * inv;
            self.b2 += (self.target_b2 - self.b2) * inv;
            self.a1 += (self.target_a1 - self.a1) * inv;
            self.a2 += (self.target_a2 - self.a2) * inv;
            self.ramp_remaining -= 1;
        }
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }

    /// Magnitude response in dB at the given frequency.
    pub fn magnitude_db(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        let w = 2.0 * PI * freq_hz / sample_rate;
        let cos_w = w.cos();
        let sin_w = w.sin();
        let cos_2w = (2.0 * w).cos();
        let sin_2w = (2.0 * w).sin();
        let nr = self.b0 + self.b1 * cos_w + self.b2 * cos_2w;
        let ni = -(self.b1 * sin_w + self.b2 * sin_2w);
        let dr = 1.0 + self.a1 * cos_w + self.a2 * cos_2w;
        let di = -(self.a1 * sin_w + self.a2 * sin_2w);
        let mag_sq = (nr * nr + ni * ni) / (dr * dr + di * di).max(1e-30);
        10.0 * mag_sq.max(1e-30_f32).log10()
    }
}

#[cfg(test)]
mod biquad_smoothing_tests {
    use super::*;

    #[test]
    fn update_coefficients_preserves_state_and_smooths_output() {
        // Issue #358 — moving an EQ band slider mid-stream used to click
        // because a fresh BiquadFilter was constructed with x1..y2 = 0 and
        // the new coefficients applied immediately. update_coefficients must
        // keep state and ramp coefficients toward target so the sample at the
        // swap boundary does not jump.
        let mut bq = BiquadFilter::new(BiquadKind::Peak, 1000.0, 0.0, 1.0, 48_000.0);
        let two_pi = 2.0 * std::f32::consts::PI;
        let mut last_before = 0.0_f32;
        for n in 0..1024 {
            let s = (two_pi * 1000.0 * n as f32 / 48_000.0).sin() * 0.5;
            last_before = bq.process(s);
        }
        bq.update_coefficients(BiquadKind::Peak, 1000.0, 12.0, 1.0, 48_000.0);
        let next_input = (two_pi * 1000.0 * 1024_f32 / 48_000.0).sin() * 0.5;
        let first_after = bq.process(next_input);
        let cliff = (first_after - last_before).abs();
        assert!(
            cliff < 0.5,
            "click on first sample after retune: |{first_after} - {last_before}| = {cliff}"
        );
    }

    #[test]
    fn update_coefficients_eventually_reaches_target() {
        // After the ramp window completes, the magnitude at center must match
        // a freshly built filter at the same params — otherwise the filter
        // would forever lag the user's slider.
        let mut bq = BiquadFilter::new(BiquadKind::Peak, 1000.0, 0.0, 1.0, 48_000.0);
        bq.update_coefficients(BiquadKind::Peak, 1000.0, 12.0, 1.0, 48_000.0);
        for _ in 0..(BIQUAD_COEFF_RAMP_FRAMES + 4) {
            bq.process(0.0);
        }
        let mag = bq.magnitude_db(1000.0, 48_000.0);
        assert!(
            (mag - 12.0).abs() < 0.5,
            "post-ramp magnitude {mag} dB not at target +12 dB"
        );
    }
}
