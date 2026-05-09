//! Generic NAM instantiation from a `plugin_loader::LoadedPackage`.
//!
//! Picks the capture file that matches the user's `ParameterSet` (axes
//! declared in the manifest) and hands it to the existing
//! [`crate::build_processor_with_assets_for_layout`].
//!
//! Issue #287 (loader) + #402 (loudness normalization).
//!
//! # Loudness normalization (issue #402)
//!
//! Every NAM block ships out of the box at the same ceiling (-1 dBFS
//! sample peak), no per-block knob, no manifest data, no offline tool.
//! The probe runs on the load path, never on the audio thread:
//!
//! 1. Build a probe processor.
//! 2. Push 1 second of pink noise (-18 LUFS) through it, measure the
//!    output's absolute sample peak.
//! 3. Compute `gain = -1 dBFS_linear / measured_peak`. Cache it
//!    keyed by capture file path.
//! 4. Build the actual processor and wrap it in a tiny gain stage that
//!    multiplies every sample by the cached factor.
//!
//! Cache hits skip the probe (~zero cost). First-load cost is ~1 second
//! per unique `.nam` capture, paid on the load thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, bail, Result};
use block_core::param::ParameterSet;
use block_core::{
    AudioChannelLayout, BlockProcessor, MonoProcessor, StereoProcessor,
};
use plugin_loader::manifest::Backend;
use plugin_loader::LoadedPackage;

use crate::build_processor_with_assets_for_layout;
use crate::processor::{plugin_params_from_set_with_defaults, DEFAULT_PLUGIN_PARAMS};

/// Sample-peak ceiling every NAM is allowed to reach (issue #402).
///
/// Conservative — quieter than -1 / -3 dBFS, so high-gain stacks don't
/// blow up when chained (TS9 → Bogner caused acoustic feedback at -3
/// dBFS). The normalization NEVER amplifies; gains > 1.0 are clamped
/// to unity. NAMs already quieter than the target keep their natural
/// level — only the loud ones are pulled down to match. Result: all
/// NAMs sit at or below the same ceiling, nothing adds gain.
const TARGET_PEAK_DBFS: f32 = -6.0;

/// Pink noise reference signal level — chosen to mirror real playing
/// level, so high-gain preamps actually saturate during the probe and
/// expose a representative peak. -18 LUFS was too quiet: high-gain
/// preamps under-measured peak and got over-amplified at play time.
const PROBE_REFERENCE_LUFS: f32 = -12.0;

/// Probe length. 2 s at 48 kHz = 96 000 samples per capture. Long
/// enough that envelope followers and slow saturation stages settle.
const PROBE_DURATION_SAMPLES: usize = 96_000;

/// Per-capture gain cache keyed by the absolute path of the `.nam`
/// file. Process-global, lives until exit.
fn gain_cache() -> &'static Mutex<HashMap<PathBuf, f32>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, f32>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Build a [`BlockProcessor`] from a disk-backed NAM package.
///
/// Wraps the inner processor in a loudness-normalizing gain stage so
/// every NAM in the catalogue lands at the same -1 dBFS ceiling
/// regardless of the capture's baked level (issue #402).
pub fn build_from_package(
    package: &LoadedPackage,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let (parameters, captures) = match &package.manifest.backend {
        Backend::Nam {
            parameters,
            captures,
        } => (parameters, captures),
        _ => bail!(
            "nam::build_from_package called with non-NAM backend (model `{}`)",
            package.manifest.id
        ),
    };
    let capture = plugin_loader::dispatch::resolve_capture(parameters, captures, params)
        .ok_or_else(|| {
            anyhow!(
                "no NAM capture matches user params for `{}`",
                package.manifest.id
            )
        })?;
    let model_path = package.root.join(&capture.file);
    let model_path_str = model_path
        .to_str()
        .ok_or_else(|| anyhow!("non-utf8 capture path: {model_path:?}"))?;
    let plugin_params = plugin_params_from_set_with_defaults(params, DEFAULT_PLUGIN_PARAMS)?;

    let gain = cached_or_measure_gain(&model_path, model_path_str, sample_rate, layout)?;
    let inner =
        build_processor_with_assets_for_layout(model_path_str, None, plugin_params, sample_rate, layout)?;
    Ok(wrap_with_gain(inner, gain))
}

fn cached_or_measure_gain(
    cache_key: &Path,
    model_path_str: &str,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<f32> {
    if let Some(g) = gain_cache().lock().unwrap().get(cache_key).copied() {
        return Ok(g);
    }
    let probe = build_processor_with_assets_for_layout(
        model_path_str,
        None,
        DEFAULT_PLUGIN_PARAMS,
        sample_rate,
        layout,
    )?;
    let pink = pink_noise_at(PROBE_REFERENCE_LUFS, PROBE_DURATION_SAMPLES);
    let peak = measure_peak(probe, &pink);
    let gain = compute_gain_to_target(peak, TARGET_PEAK_DBFS);
    let gain_db = 20.0 * gain.log10();
    let peak_db = if peak > 1e-9 { 20.0 * peak.log10() } else { f32::NEG_INFINITY };
    log::info!(
        "nam loudness: {} -> raw peak {peak_db:+.2} dBFS, gain {gain_db:+.2} dB",
        cache_key.display()
    );
    gain_cache()
        .lock()
        .unwrap()
        .insert(cache_key.to_path_buf(), gain);
    Ok(gain)
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

/// Linear gain that brings a measured `peak` down to the `target_dbfs`
/// ceiling — but **never amplifies**. NAMs already quieter than the
/// target keep their natural level (gain clamped to 1.0); only loud
/// NAMs are attenuated. Pure function, cache-friendly, easy to test.
///
/// Why never amplify: chained NAMs (e.g. NAM gain pedal → NAM amp)
/// would otherwise stack their boosts and blow up — the user reported
/// acoustic feedback when the chain was TS9 → Bogner Ecstasy at a
/// lower (more aggressive) target. Attenuation-only keeps every NAM
/// at-or-below the ceiling without adding energy to the signal path.
pub fn compute_gain_to_target(peak: f32, target_dbfs: f32) -> f32 {
    if peak <= 1e-9 {
        // Capture stuck at silence — leave it alone, never amplify noise.
        return 1.0;
    }
    let target_linear = 10f32.powf(target_dbfs / 20.0);
    let raw = target_linear / peak;
    raw.min(1.0)
}

fn wrap_with_gain(inner: BlockProcessor, gain: f32) -> BlockProcessor {
    match inner {
        BlockProcessor::Mono(p) => BlockProcessor::Mono(Box::new(GainMono {
            inner: p,
            gain,
        })),
        BlockProcessor::Stereo(p) => BlockProcessor::Stereo(Box::new(GainStereo {
            inner: p,
            gain,
        })),
    }
}

struct GainMono {
    inner: Box<dyn MonoProcessor>,
    gain: f32,
}

impl MonoProcessor for GainMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.inner.process_sample(input) * self.gain
    }
}

struct GainStereo {
    inner: Box<dyn StereoProcessor>,
    gain: f32,
}

impl StereoProcessor for GainStereo {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let [l, r] = self.inner.process_frame(input);
        [l * self.gain, r * self.gain]
    }
}

/// Voss-McCartney pink noise normalized so the integrated RMS over the
/// requested duration matches `target_lufs`. Deterministic seed for
/// reproducibility — every probe of the same capture sees the same
/// stimulus and produces the same gain.
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

/// Register this crate's builder in the global package-builders table.
pub fn register_builder() {
    plugin_loader::package_builders::register(
        plugin_loader::package_builders::BackendKind::Nam,
        build_from_package,
    );
}

#[cfg(test)]
#[path = "from_package_tests.rs"]
mod tests;
