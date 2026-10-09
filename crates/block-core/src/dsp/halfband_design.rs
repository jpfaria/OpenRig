//! Responsibility: computes the allpass coefficients of a polyphase IIR half-band low-pass.
//!
//! Elliptic half-band design after Laurent de Soras' HIIR: the filter is
//! the sum of two chains of first-order allpasses running at half the
//! rate, so it costs a handful of multiplies per sample and adds only a
//! few samples of delay (no linear-phase FIR latency).

use std::f64::consts::PI;

/// Coefficients one filter may hold (two chains of six allpasses).
pub const MAX_COEFS: usize = 12;

#[derive(Debug, Clone, Copy)]
pub struct HalfbandCoefs {
    pub coefs: [f32; MAX_COEFS],
    pub len: usize,
}

/// Coefficients for a half-band low-pass with `attenuation_db` of stopband
/// rejection and a transition band of `transition` (fraction of the
/// doubled rate, 0..0.5): the passband ends at `0.25 - transition` and the
/// stopband starts at `0.25 + transition`. A spec that needs more than
/// [`MAX_COEFS`] gets the best filter that fits.
pub fn design(attenuation_db: f64, transition: f64) -> HalfbandCoefs {
    let transition = transition.clamp(1e-4, 0.4999);
    let (k, q) = transition_params(transition);
    let order = filter_order(attenuation_db, q).min(MAX_COEFS * 2 + 1);
    let len = (order - 1) / 2;
    let mut coefs = [0.0_f32; MAX_COEFS];
    for (index, slot) in coefs.iter_mut().take(len).enumerate() {
        *slot = coefficient(index + 1, k, q, order) as f32;
    }
    HalfbandCoefs { coefs, len }
}

fn transition_params(transition: f64) -> (f64, f64) {
    let k = ((1.0 - transition * 2.0) * PI / 4.0).tan().powi(2);
    let kksqrt = (1.0 - k * k).powf(0.25);
    let e = 0.5 * (1.0 - kksqrt) / (1.0 + kksqrt);
    let e4 = e.powi(4);
    let q = e * (1.0 + e4 * (2.0 + e4 * (15.0 + 150.0 * e4)));
    (k, q)
}

fn filter_order(attenuation_db: f64, q: f64) -> usize {
    let attn = 10f64.powf(-attenuation_db / 10.0);
    let a = attn / (1.0 - attn);
    let mut order = ((a * a / 16.0).ln() / q.ln()).ceil().max(3.0) as usize;
    if order % 2 == 0 {
        order += 1;
    }
    order
}

fn coefficient(c: usize, k: f64, q: f64, order: usize) -> f64 {
    let c = c as f64;
    let order = order as f64;
    let mut num = 0.0;
    let mut i = 0_i32;
    loop {
        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
        let term = q.powi(i * (i + 1)) * ((2 * i + 1) as f64 * c * PI / order).sin() * sign;
        num += term;
        i += 1;
        if term.abs() < 1e-100 || i > 64 {
            break;
        }
    }
    let mut den = 0.0;
    let mut i = 1_i32;
    loop {
        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
        let term = q.powi(i * i) * ((2 * i) as f64 * c * PI / order).cos() * sign;
        den += term;
        i += 1;
        if term.abs() < 1e-100 || i > 64 {
            break;
        }
    }
    let ww = num * q.powf(0.25) / (den + 0.5);
    let wwsq = ww * ww;
    let x = ((1.0 - wwsq * k) * (1.0 - wwsq / k)).sqrt() / (1.0 + wwsq);
    (1.0 - x) / (1.0 + x)
}

#[cfg(test)]
#[path = "halfband_design_tests.rs"]
mod tests;
