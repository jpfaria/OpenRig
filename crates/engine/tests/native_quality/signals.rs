//! Responsibility: generates the deterministic test signals of the battery.

/// FFT size every spectral measurement uses. A tone placed on an exact bin
/// of this size has no leakage, so the window is the only skirt.
pub const FFT_SIZE: usize = 16_384;

/// Frequency of FFT bin `bin` at `sample_rate`.
pub fn bin_hz(bin: usize, sample_rate: f32) -> f32 {
    bin as f32 * sample_rate / FFT_SIZE as f32
}

/// A sine of `freq` Hz at peak `amp`, `len` samples.
pub fn sine(len: usize, freq: f32, sample_rate: f32, amp: f32) -> Vec<f32> {
    (0..len)
        .map(|i| {
            (std::f64::consts::TAU * freq as f64 * i as f64 / sample_rate as f64).sin() as f32 * amp
        })
        .collect()
}

/// `burst_len` samples of seeded white noise at peak `amp`, then silence up
/// to `total_len`.
pub fn noise_burst(total_len: usize, burst_len: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut out = vec![0.0; total_len];
    let mut state = seed | 1;
    for slot in out.iter_mut().take(burst_len.min(total_len)) {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let unit = (state >> 40) as f32 / (1u64 << 24) as f32;
        *slot = (unit * 2.0 - 1.0) * amp;
    }
    out
}

/// Linear amplitude of `db` dBFS.
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}
