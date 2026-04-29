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
pub struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

pub enum BiquadKind {
    Peak,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
    Notch,
}

impl BiquadFilter {
    pub fn new(kind: BiquadKind, freq_hz: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
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
        Self {
            b0: b0 * inv_a0,
            b1: b1 * inv_a0,
            b2: b2 * inv_a0,
            a1: a1 * inv_a0,
            a2: a2 * inv_a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
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
