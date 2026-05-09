//! Backend-agnostic loudness normalization for every plugin package
//! (issue #402).
//!
//! Every NAM / IR / LV2 / VST3 capture lands at the same sample peak
//! (-18 dBFS by default) on first build. The probe runs on the load
//! thread, never on the audio thread, and is cached per `manifest.id`
//! so re-loads cost nothing.
//!
//! # Design notes
//!
//! - **Same target across backends.** The user wants every block at
//!   the same level — NAM amp, IR cab, LV2 reverb. The same probe
//!   procedure works for all because [`BlockProcessor`] is the only
//!   abstraction we need.
//! - **Conservative ceiling (-18 dBFS).** Chained gain stages add up
//!   (NAM gain pedal → NAM amp → LV2 reverb). A low ceiling leaves
//!   room for the stack to grow without exceeding 0 dBFS or causing
//!   acoustic feedback. The user can compensate the resulting low
//!   chain output via the `volume` block at the end of the chain.
//! - **Pink noise probe.** Voss-McCartney generator at -12 LUFS RMS,
//!   2 s long. Loud enough that high-gain saturation stages actually
//!   show a representative peak; deterministic seed so the same
//!   capture always computes the same gain.
//! - **No knob, no manifest data.** Per the user: "tudo no mesmo
//!   volume, sempre 100%".

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use block_core::{BlockProcessor, MonoProcessor, StereoProcessor};

/// Sample-peak target. Conservative; see module docs.
const TARGET_PEAK_DBFS: f32 = -18.0;

/// Pink noise probe level — chosen to mirror real playing level so
/// high-gain stages saturate as they would under play.
const PROBE_REFERENCE_LUFS: f32 = -12.0;

/// 2 s at 48 kHz. Long enough for slow envelope followers to settle.
const PROBE_DURATION_SAMPLES: usize = 96_000;

/// Per-package gain cache keyed by `manifest.id`. Process-global,
/// lives until exit.
fn gain_cache() -> &'static Mutex<HashMap<String, f32>> {
    static CACHE: OnceLock<Mutex<HashMap<String, f32>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Probe `probe_processor`, compute and cache the gain, then wrap
/// `real_processor` in a gain stage that brings every sample down to
/// the [`TARGET_PEAK_DBFS`] target. The probe processor is consumed.
///
/// `cache_key` should be unique per package — typically `manifest.id`.
/// The two processors must be built from the same package + params so
/// the probe's measurement applies to the real processor.
pub fn normalize(
    cache_key: &str,
    probe_processor: BlockProcessor,
    real_processor: BlockProcessor,
) -> BlockProcessor {
    let gain = cached_or_measure(cache_key, probe_processor);
    wrap_with_gain(real_processor, gain)
}

fn cached_or_measure(cache_key: &str, probe: BlockProcessor) -> f32 {
    if let Some(g) = gain_cache().lock().unwrap().get(cache_key).copied() {
        return g;
    }
    let pink = pink_noise_at(PROBE_REFERENCE_LUFS, PROBE_DURATION_SAMPLES);
    let peak = measure_peak(probe, &pink);
    let gain = compute_gain_to_target(peak, TARGET_PEAK_DBFS);
    let gain_db = 20.0 * gain.max(1e-9).log10();
    let peak_db = if peak > 1e-9 {
        20.0 * peak.log10()
    } else {
        f32::NEG_INFINITY
    };
    log::info!(
        "loudness norm: {cache_key} -> raw peak {peak_db:+.2} dBFS, gain {gain_db:+.2} dB"
    );
    gain_cache()
        .lock()
        .unwrap()
        .insert(cache_key.to_string(), gain);
    gain
}

fn measure_peak(mut processor: BlockProcessor, pink: &[f32]) -> f32 {
    let mut peak: f32 = 0.0;
    match &mut processor {
        BlockProcessor::Mono(p) => {
            for &x in pink {
                let y = p.process_sample(x).abs();
                if y > peak {
                    peak = y;
                }
            }
        }
        BlockProcessor::Stereo(p) => {
            for &x in pink {
                let [l, r] = p.process_frame([x, x]);
                let y = l.abs().max(r.abs());
                if y > peak {
                    peak = y;
                }
            }
        }
    }
    peak
}

/// Linear gain that brings `peak` to the `target_dbfs` ceiling.
/// Pure function — easy to test, no I/O.
pub fn compute_gain_to_target(peak: f32, target_dbfs: f32) -> f32 {
    if peak <= 1e-9 {
        return 1.0;
    }
    let target_linear = 10f32.powf(target_dbfs / 20.0);
    target_linear / peak
}

fn wrap_with_gain(inner: BlockProcessor, gain: f32) -> BlockProcessor {
    let ceiling = 10f32.powf(TARGET_PEAK_DBFS / 20.0);
    match inner {
        BlockProcessor::Mono(p) => BlockProcessor::Mono(Box::new(GainMono {
            inner: p,
            gain,
            ceiling,
        })),
        BlockProcessor::Stereo(p) => BlockProcessor::Stereo(Box::new(GainStereo {
            inner: p,
            gain,
            ceiling,
        })),
    }
}

/// Soft-saturate `x` to never exceed `ceiling`, smooth around the
/// boundary so chained blocks don't bring out hard-clip clicks. Uses
/// `x / (1 + |x|)` (rational tanh approximation) — single-sample,
/// cheap on ARM, asymptotic to ±1 then scaled to ±ceiling. Inputs
/// well below the ceiling pass through nearly unchanged.
#[inline]
fn soft_clip_to_ceiling(x: f32, ceiling: f32) -> f32 {
    if ceiling <= 1e-9 {
        return 0.0;
    }
    let normalized = x / ceiling;
    let saturated = normalized / (1.0 + normalized.abs());
    saturated * ceiling
}

struct GainMono {
    inner: Box<dyn MonoProcessor>,
    gain: f32,
    ceiling: f32,
}

impl MonoProcessor for GainMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let y = self.inner.process_sample(input) * self.gain;
        soft_clip_to_ceiling(y, self.ceiling)
    }
}

struct GainStereo {
    inner: Box<dyn StereoProcessor>,
    gain: f32,
    ceiling: f32,
}

impl StereoProcessor for GainStereo {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let [l, r] = self.inner.process_frame(input);
        [
            soft_clip_to_ceiling(l * self.gain, self.ceiling),
            soft_clip_to_ceiling(r * self.gain, self.ceiling),
        ]
    }
}

/// Voss-McCartney pink noise normalized to `target_lufs` RMS.
/// Deterministic seed — the same capture always produces the same gain.
fn pink_noise_at(target_lufs: f32, n_samples: usize) -> Vec<f32> {
    let raw = pink_noise_voss(n_samples);
    let r = rms(&raw).max(1e-12);
    let scale = lufs_to_linear(target_lufs) / r;
    raw.iter().map(|s| s * scale).collect()
}

fn pink_noise_voss(n: usize) -> Vec<f32> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        ((state >> 11) as f32 / u32::MAX as f32) - 0.5
    };
    let octaves = 7;
    let mut rows = vec![0.0_f32; octaves];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let trailing = (i as u32).trailing_zeros() as usize;
        if trailing < rows.len() {
            rows[trailing] = next();
        }
        out.push(rows.iter().sum::<f32>() / octaves as f32);
    }
    out
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

fn lufs_to_linear(lufs: f32) -> f32 {
    10f32.powf(lufs / 20.0)
}

#[cfg(test)]
#[path = "loudness_norm_tests.rs"]
mod tests;
