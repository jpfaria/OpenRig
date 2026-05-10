//! Per-NAM loudness probe (issue #402).
//!
//! Roda 1x na construção do `NamProcessor`: gera pink noise determinístico,
//! processa pelo modelo NAM já carregado, mede o pico de saída e devolve
//! quanto somar em dB pra alinhar todos os NAMs num mesmo target peak.
//!
//! Cacheado em memória por `model_path` na sessão. NUNCA roda no audio
//! thread (apenas em `NamProcessor::new`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use block_core::lin_to_db;

use crate::processor::{nam_process, NeuralModel};

/// Loudness target. The probe boosts each NAM until its OUTPUT RMS
/// hits this level — what the ear actually perceives as "volume".
/// Two captures with the same peak can have very different RMS; peak
/// alone fails to nivel them.
pub const TARGET_RMS_DBFS: f32 = -16.0;

/// Hard ceiling for the OUTPUT sample peak after the probe gain is
/// applied. Caps the boost when the loudness target would push the
/// peak past this ceiling — "no maximo sem clipar". The chain's
/// brickwall limiter catches anything that still slips through on
/// real guitar transients.
pub const PEAK_CEILING_DBFS: f32 = -1.0;

/// Peak amplitude of the pink-noise probe at the model input. Picked
/// to roughly mirror "instrument-level" guitar peaks so the model
/// saturates the way it would in real use.
pub const PROBE_INPUT_PEAK_DBFS: f32 = -12.0;

pub const PROBE_SAMPLES: usize = 96_000;

/// BOOST-ONLY: a NAM already baked at or above the loudness target
/// is left alone. Probe never attenuates.
pub const MIN_OFFSET_DB: f32 = 0.0;
pub const MAX_OFFSET_DB: f32 = 24.0;

const PINK_OCTAVES: usize = 8;
const PROBE_SEED: u64 = 0xC0FFEE;
const PEAK_FLOOR_DBFS: f32 = -120.0;

fn cache() -> &'static Mutex<HashMap<String, f32>> {
    static CACHE: OnceLock<Mutex<HashMap<String, f32>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Look up cached offset for `model_path`, probing the model on first use.
///
/// SAFETY: `model` must be a live pointer returned by the NAM lib.
pub unsafe fn compute_or_lookup(model_path: &str, model: *mut NeuralModel) -> f32 {
    if let Some(cached) = lookup_cached(model_path) {
        return cached;
    }
    let offset = probe_model(model);
    cache()
        .lock()
        .unwrap()
        .insert(model_path.to_string(), offset);
    offset
}

unsafe fn probe_model(model: *mut NeuralModel) -> f32 {
    let input = pink_noise_buffer(PROBE_SAMPLES, PROBE_SEED);
    let mut output = vec![0.0_f32; PROBE_SAMPLES];
    nam_process(model, &input, &mut output);
    compute_offset_db(rms_dbfs(&output), peak_dbfs(&output))
}

fn pink_noise_buffer(samples: usize, seed: u64) -> Vec<f32> {
    let mut rng = XorShift64::new(seed);
    let mut rolls = [0.0_f32; PINK_OCTAVES];
    for r in rolls.iter_mut() {
        *r = rng.next_f32_signed();
    }
    let mut buf = Vec::with_capacity(samples);
    for n in 0..samples {
        for (i, r) in rolls.iter_mut().enumerate() {
            if (n as u64) & (1u64 << i) == 0 {
                *r = rng.next_f32_signed();
            }
        }
        let pink = rolls.iter().sum::<f32>() + rng.next_f32_signed();
        buf.push(pink);
    }
    normalize_to_peak_dbfs(&mut buf, PROBE_INPUT_PEAK_DBFS);
    buf
}

fn normalize_to_peak_dbfs(buf: &mut [f32], target_dbfs: f32) {
    let peak = buf.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    if peak == 0.0 {
        return;
    }
    let target_lin = 10.0_f32.powf(target_dbfs / 20.0);
    let scale = target_lin / peak;
    for s in buf.iter_mut() {
        *s *= scale;
    }
}

fn peak_dbfs(buf: &[f32]) -> f32 {
    let peak = buf.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    if peak == 0.0 {
        PEAK_FLOOR_DBFS
    } else {
        lin_to_db(peak)
    }
}

fn rms_dbfs(buf: &[f32]) -> f32 {
    let mean_sq = buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32;
    if mean_sq == 0.0 {
        PEAK_FLOOR_DBFS
    } else {
        10.0 * mean_sq.log10()
    }
}

/// Pick the smaller of (boost-to-loudness-target, boost-up-to-peak-ceiling).
/// `measured_rms_dbfs` and `measured_peak_dbfs` come from the same probed
/// output buffer.
fn compute_offset_db(measured_rms_dbfs: f32, measured_peak_dbfs: f32) -> f32 {
    let want_for_loudness = TARGET_RMS_DBFS - measured_rms_dbfs;
    let allowed_by_peak = PEAK_CEILING_DBFS - measured_peak_dbfs;
    want_for_loudness
        .min(allowed_by_peak)
        .clamp(MIN_OFFSET_DB, MAX_OFFSET_DB)
}

#[cfg(test)]
fn lookup_cached(model_path: &str) -> Option<f32> {
    cache().lock().unwrap().get(model_path).copied()
}

#[cfg(not(test))]
fn lookup_cached(model_path: &str) -> Option<f32> {
    cache().lock().unwrap().get(model_path).copied()
}

#[cfg(test)]
fn insert_for_test(model_path: &str, offset_db: f32) {
    cache()
        .lock()
        .unwrap()
        .insert(model_path.to_string(), offset_db);
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
        let v = self.next_u64() as f64 / u64::MAX as f64;
        (v as f32) * 2.0 - 1.0
    }
}

#[cfg(test)]
#[path = "loudness_probe_tests.rs"]
mod tests;
