//! Responsibility: generates the reference guitar DI every capture level is measured with.
//!
//! Exact port of OpenRig-plugins `tools/loudness_audit/src/synthetic_di.rs`,
//! so a tone installed from TONE3000 lands at the same level as the curated
//! catalog: three Karplus-Strong power chords, 12 s at 48 kHz, peak -15 dBFS.

const SAMPLE_RATE: f32 = 48_000.0;

pub const DI_SAMPLE_RATE: f32 = SAMPLE_RATE;

pub fn default_guitar_di() -> Vec<f32> {
    let chords = &[(82.41, 0.0, 4.0), (110.0, 4.0, 4.0), (146.83, 8.0, 4.0)];
    synth_chords(12.0, chords, -15.0)
}

fn synth_chords(total_seconds: f32, chords: &[(f32, f32, f32)], peak_dbfs: f32) -> Vec<f32> {
    let total = (total_seconds * SAMPLE_RATE) as usize;
    let mut buf = vec![0.0_f32; total];
    for &(root_hz, start_s, length_s) in chords {
        let start = (start_s * SAMPLE_RATE) as usize;
        let length = ((length_s * SAMPLE_RATE) as usize).min(total.saturating_sub(start));
        let chord = power_chord(root_hz, length);
        for (i, s) in chord.iter().enumerate() {
            buf[start + i] += *s;
        }
    }
    normalize_peak_dbfs(&mut buf, peak_dbfs);
    buf
}

/// Root, fifth and octave, re-plucked every second.
fn power_chord(root_hz: f32, samples: usize) -> Vec<f32> {
    let voicings = [
        (root_hz, 1.0_f32),
        (root_hz * 1.5, 0.85),
        (root_hz * 2.0, 0.7),
    ];
    let pluck_period = SAMPLE_RATE as usize;
    let mut out = vec![0.0_f32; samples];
    let mut start = 0usize;
    while start < samples {
        let len = (samples - start).min(pluck_period);
        for &(hz, gain) in &voicings {
            let pluck = karplus_strong(hz, len, start as u64);
            for (i, s) in pluck.iter().enumerate() {
                out[start + i] += s * gain;
            }
        }
        start += pluck_period;
    }
    apply_envelope(&mut out);
    out
}

fn karplus_strong(freq_hz: f32, samples: usize, seed_salt: u64) -> Vec<f32> {
    let buf_len = (SAMPLE_RATE / freq_hz).round().max(2.0) as usize;
    let mut delay = vec![0.0_f32; buf_len];
    let seed = 0xC0FFEE_u64
        .wrapping_mul((freq_hz * 100.0) as u64 + 1)
        .wrapping_add(seed_salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut rng = XorShift64::new(seed);
    for s in delay.iter_mut() {
        *s = rng.next_f32_signed();
    }
    let mut out = vec![0.0_f32; samples];
    let mut idx = 0;
    for o in out.iter_mut() {
        let next = (idx + 1) % buf_len;
        *o = delay[idx];
        delay[idx] = (delay[idx] + delay[next]) * 0.5;
        idx = next;
    }
    out
}

/// 5 ms linear attack so the chord does not open with a click.
fn apply_envelope(buf: &mut [f32]) {
    let attack_samples = (SAMPLE_RATE * 0.005) as usize;
    for (i, s) in buf.iter_mut().enumerate().take(attack_samples) {
        *s *= i as f32 / attack_samples as f32;
    }
}

fn normalize_peak_dbfs(buf: &mut [f32], target_dbfs: f32) {
    let peak = buf.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    if peak == 0.0 {
        return;
    }
    let scale = 10.0_f32.powf(target_dbfs / 20.0) / peak;
    for s in buf.iter_mut() {
        *s *= scale;
    }
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xDEAD_BEEF } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_f32_signed(&mut self) -> f32 {
        (self.next_u64() as f64 / u64::MAX as f64) as f32 * 2.0 - 1.0
    }
}
