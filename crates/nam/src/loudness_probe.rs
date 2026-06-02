//! Per-NAM loudness probe (issue #402).
//!
//! Roda 1x na construção do `NamProcessor`: gera pink noise determinístico,
//! processa pelo modelo NAM já carregado, mede o pico de saída e devolve
//! quanto somar em dB pra alinhar todos os NAMs num mesmo target peak.
//!
//! Cacheado em memória por `model_path` na sessão. NUNCA roda no audio
//! thread (apenas em `NamProcessor::new`).

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
use block_core::lin_to_db;

// Issue #612: the FFI-driven probe (`compute_or_lookup` / `probe_model`
// / `diagnose_model`) was removed when the FFI moved to the official
// `nam_wrapper` C entrypoints, which expose no per-model `Process`
// handle for offline probing. Loudness alignment is metadata-driven now
// (`manifest.output_gain_db`, populated offline by `tools/nam_loudness_audit`
// and read via `baked_loudness`), so the runtime never probed the model
// anyway. The pure measurement math below stays — it is the engine the
// audit tooling uses and is covered by FFI-free tests.

/// Loudness target — set to where the loudest hot captures sit
/// naturally (Bogner Ecstasy & friends measure ~ -10 dBFS RMS on the
/// pink-noise probe). Lower targets meant the probe never boosted the
/// quiet captures up to where the hot ones already were.
pub const TARGET_RMS_DBFS: f32 = -10.0;

/// Sample-peak ceiling AFTER the probe gain is applied. Set well
/// above 0 dBFS so the loudness target is reachable for clean amps
/// with high crest factor (ex: Dumble Steel-String Singer needs ~7 dB
/// of boost; with a -1 ceiling the peak constraint clamps boost long
/// before RMS catches up). The chain's brickwall limiter catches the
/// residual transients.
pub const PEAK_CEILING_DBFS: f32 = 3.0;

/// Peak amplitude of the pink-noise probe at the model input. Hits
/// at -25 dBFS porque é o nível típico que o amp recebe NA CHAIN
/// REAL: signal de guitarra (~-15 dBFS peak no input device) passa
/// por um gain pedal upstream (Klon, TS9, etc.) que tipicamente
/// REDUZ o peak antes do amp ver. Calibrar o probe a -12 dBFS (que
/// é signal denso direto) faz audit subestimar bastante o gain
/// necessário, e o amp acaba saindo mais quieto do que o target
/// promete (issue #413).
pub const PROBE_INPUT_PEAK_DBFS: f32 = -25.0;

pub const PROBE_SAMPLES: usize = 96_000;

/// BOOST-ONLY: a NAM already baked at or above the loudness target
/// is left alone. Probe never attenuates.
pub const MIN_OFFSET_DB: f32 = 0.0;
pub const MAX_OFFSET_DB: f32 = 24.0;

// Issue #612: with the FFI-driven probe gone, the pink-noise generator
// and dBFS measurement helpers are exercised only by the FFI-free unit
// tests that pin their math (they remain the reference engine the
// offline audit tooling reuses). Gate them to `test` so production
// builds stay warning-free without deleting the measurement spec.
#[cfg(test)]
const PINK_OCTAVES: usize = 8;
#[cfg(test)]
const PEAK_FLOOR_DBFS: f32 = -120.0;

#[cfg(test)]
fn cache() -> &'static Mutex<HashMap<String, f32>> {
    static CACHE: OnceLock<Mutex<HashMap<String, f32>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn peak_dbfs(buf: &[f32]) -> f32 {
    let peak = buf.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    if peak == 0.0 {
        PEAK_FLOOR_DBFS
    } else {
        lin_to_db(peak)
    }
}

#[cfg(test)]
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
#[cfg(test)]
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

#[cfg(test)]
fn insert_for_test(model_path: &str, offset_db: f32) {
    cache()
        .lock()
        .unwrap()
        .insert(model_path.to_string(), offset_db);
}

#[cfg(test)]
struct XorShift64 {
    state: u64,
}

#[cfg(test)]
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
