//! Generic NAM instantiation from a `plugin_loader::LoadedPackage`.
//!
//! Picks the capture file that matches the user's `ParameterSet` (axes
//! declared in the manifest) and hands it to the existing
//! [`crate::build_processor_with_assets_for_layout`].
//!
//! Issue: #287

use anyhow::{anyhow, bail, Result};
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor, StereoProcessor};
use plugin_loader::manifest::{Backend, GridParameter};
use plugin_loader::LoadedPackage;

use crate::build_processor_with_assets_for_layout;
use crate::processor::{plugin_params_from_set_with_defaults, DEFAULT_PLUGIN_PARAMS};

/// True passthrough for the NAM block when the user knob is at zero.
/// Issue #400 follow-up: -60 dB attenuation via `output_level_db` was not
/// enough to suppress the NAM model's bias/noise once the downstream AMP
/// (Mesa Rectifier `drive_red` etc.) amplified it back into audible territory.
/// Returning a passthrough makes the block behave EXACTLY as if it were
/// disabled — the user-validated baseline that doesn't produce microphonics.
struct PassthroughMono;
impl MonoProcessor for PassthroughMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        input
    }
    fn process_block(&mut self, _buffer: &mut [f32]) {
        // Identity: leave buffer untouched.
    }
}

struct PassthroughStereo;
impl StereoProcessor for PassthroughStereo {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        input
    }
    fn process_block(&mut self, _buffer: &mut [[f32; 2]]) {
        // Identity: leave buffer untouched.
    }
}

fn passthrough(layout: AudioChannelLayout) -> BlockProcessor {
    match layout {
        AudioChannelLayout::Mono => BlockProcessor::Mono(Box::new(PassthroughMono)),
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(PassthroughStereo)),
    }
}

/// Build a [`BlockProcessor`] from a disk-backed NAM package.
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
    // Issue #400 follow-up: if the user knob is at zero, return a true
    // passthrough WITHOUT loading or running the NAM model. The model's
    // internal bias/noise — even attenuated by -60 dB on output_gain —
    // was being amplified back into audible territory by downstream
    // high-gain amps, causing acoustic feedback that the user could not
    // silence by lowering knobs. Passthrough = identical behavior to a
    // disabled block (the only configuration the user validated as
    // microphonics-free).
    let level_pct = normalized_knob("level", params, parameters);
    let drive_pct = normalized_knob("drive", params, parameters);
    if level_pct.map(|p| p <= 0.0).unwrap_or(false)
        || drive_pct.map(|p| p <= 0.0).unwrap_or(false)
    {
        return Ok(passthrough(layout));
    }

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
    let mut plugin_params = plugin_params_from_set_with_defaults(params, DEFAULT_PLUGIN_PARAMS)?;

    // Issue #400 bug #1: NAM gain pedals declare `drive`/`tone`/`level`
    // params used to PICK A CAPTURE (resolve_capture above), but the
    // capture grid is sparse — for the TS9 we have 9 captures across a
    // declared 11×11×11 grid, so most user values fall back to a
    // "closest-neighbor" capture that may NOT honor user intent (e.g.
    // user wants drive=0/level=0 → expects silence; resolver picks the
    // closest capture which may have level=6 baked in → audible output
    // → microphonics on guitar pickup → feedback loop).
    //
    // Apply the user's `level` percent as a post-NAM output gain, and
    // `drive` percent as a pre-NAM input gain, so the user's knob is
    // honored regardless of which capture got picked. Logarithmic taper
    // matches the volume block (issue #400 bug #3): 0 → silence,
    // max → 0 dB unity (capture's own behavior preserved).
    if let Some(db) = derive_db_from_knob("level", params, parameters) {
        plugin_params.output_level_db = db;
    }
    if let Some(db) = derive_db_from_knob("drive", params, parameters) {
        plugin_params.input_level_db = db;
    }

    build_processor_with_assets_for_layout(model_path_str, None, plugin_params, sample_rate, layout)
}

/// User's value normalized to [0.0, 1.0] against the schema's declared max.
/// Returns `None` if the knob is missing from the schema or absent in user
/// params (so callers can fall back to defaults).
fn normalized_knob(
    name: &str,
    params: &ParameterSet,
    parameters: &[GridParameter],
) -> Option<f64> {
    let user_value = f64::from(params.get_f32(name)?);
    let max_declared = parameters
        .iter()
        .find(|p| p.name == name)?
        .values
        .iter()
        .filter_map(|v| match v {
            plugin_loader::manifest::ParameterValue::Number(n) => Some(*n),
            _ => None,
        })
        .fold(f64::NEG_INFINITY, f64::max);
    if max_declared <= 0.0 || !max_declared.is_finite() {
        return None;
    }
    Some((user_value / max_declared).clamp(0.0, 1.0))
}

/// If `name` (e.g. "level" or "drive") is declared as a numeric knob axis in
/// `parameters` AND the user provided a value, returns the dB equivalent on
/// the standard logarithmic taper:
///   - 0% of declared max → -60 dB (silence floor)
///   - 50% of declared max → -6 dB
///   - 100% of declared max → 0 dB (unity, preserves the capture's own gain)
fn derive_db_from_knob(
    name: &str,
    params: &ParameterSet,
    parameters: &[GridParameter],
) -> Option<f32> {
    let normalized = normalized_knob(name, params, parameters)?;
    if normalized <= 0.0 {
        return Some(-60.0);
    }
    Some((20.0 * normalized.log10()).max(-60.0) as f32)
}

/// Register this crate's builder in the global package-builders table.
pub fn register_builder() {
    plugin_loader::package_builders::register(
        plugin_loader::package_builders::BackendKind::Nam,
        build_from_package,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::value_objects::ParameterValue as BlockParameterValue;
    use plugin_loader::manifest::ParameterValue;

    fn ts9_like_parameters() -> Vec<GridParameter> {
        // Mirrors the real `nam_ibanez_ts9` schema: drive/tone/level on a 0–10 grid.
        let values: Vec<ParameterValue> = (0..=10)
            .map(|n| ParameterValue::Number(n as f64))
            .collect();
        vec![
            GridParameter {
                name: "drive".to_string(),
                display_name: None,
                values: values.clone(),
            },
            GridParameter {
                name: "tone".to_string(),
                display_name: None,
                values: values.clone(),
            },
            GridParameter {
                name: "level".to_string(),
                display_name: None,
                values,
            },
        ]
    }

    fn params_with(level: Option<f32>, drive: Option<f32>) -> ParameterSet {
        let mut ps = ParameterSet::default();
        if let Some(l) = level {
            ps.insert("level", BlockParameterValue::Float(l));
        }
        if let Some(d) = drive {
            ps.insert("drive", BlockParameterValue::Float(d));
        }
        ps
    }

    // ── Test #1 of issue #400: NAM gain pedals respond to drive/level ──

    #[test]
    fn level_zero_returns_silence_floor() {
        let parameters = ts9_like_parameters();
        let ps = params_with(Some(0.0), None);
        let db = derive_db_from_knob("level", &ps, &parameters)
            .expect("level=0 should produce a dB value");
        assert_eq!(db, -60.0, "level=0 must hit silence floor (-60 dB)");
    }

    #[test]
    fn level_max_returns_unity() {
        let parameters = ts9_like_parameters();
        let ps = params_with(Some(10.0), None);
        let db = derive_db_from_knob("level", &ps, &parameters)
            .expect("level=10 should produce a dB value");
        assert!(
            db.abs() < 1e-3,
            "level at max should be unity (0 dB), got {db}"
        );
    }

    #[test]
    fn level_half_returns_minus_six_db() {
        // Logarithmic taper, 50% knob → -6 dB (perceptual halving).
        let parameters = ts9_like_parameters();
        let ps = params_with(Some(5.0), None);
        let db = derive_db_from_knob("level", &ps, &parameters)
            .expect("level=5 should produce a dB value");
        assert!(
            (db - (-6.0)).abs() < 0.1,
            "level at half should be ~-6 dB, got {db}"
        );
    }

    #[test]
    fn drive_zero_returns_silence_floor() {
        // Drive at 0 silences the pre-NAM input — even if the resolver
        // picks a moderate-gain capture, the NAM model receives near-zero
        // input and produces near-silence on its output.
        let parameters = ts9_like_parameters();
        let ps = params_with(None, Some(0.0));
        let db = derive_db_from_knob("drive", &ps, &parameters)
            .expect("drive=0 should produce a dB value");
        assert_eq!(db, -60.0);
    }

    #[test]
    fn drive_and_level_produce_independent_dbs() {
        // Verify the two knobs map to separate input/output gains.
        let parameters = ts9_like_parameters();
        let ps = params_with(Some(7.0), Some(3.0));
        let level_db = derive_db_from_knob("level", &ps, &parameters).expect("level db");
        let drive_db = derive_db_from_knob("drive", &ps, &parameters).expect("drive db");
        // 7/10 = 0.7 → 20*log10(0.7) ≈ -3.10 dB
        // 3/10 = 0.3 → 20*log10(0.3) ≈ -10.46 dB
        assert!((level_db - (-3.10)).abs() < 0.1, "level db = {level_db}");
        assert!((drive_db - (-10.46)).abs() < 0.1, "drive db = {drive_db}");
    }

    #[test]
    fn knob_absent_from_user_params_returns_none() {
        // If user did NOT set `level` (params doesn't contain the key),
        // we return None so any explicit `output_db` keeps effect.
        let parameters = ts9_like_parameters();
        let ps = params_with(None, None);
        assert!(derive_db_from_knob("level", &ps, &parameters).is_none());
        assert!(derive_db_from_knob("drive", &ps, &parameters).is_none());
    }

    #[test]
    fn knob_absent_from_schema_returns_none() {
        // If the manifest does NOT declare a `gain_percent` axis but the user
        // happens to set one in YAML, we return None — schema is authority.
        let parameters = ts9_like_parameters();
        let mut ps = ParameterSet::default();
        ps.insert("gain_percent", BlockParameterValue::Float(50.0));
        assert!(derive_db_from_knob("gain_percent", &ps, &parameters).is_none());
    }

    // ── Passthrough tests (microphonics fix) ──────────────────────────

    #[test]
    fn passthrough_mono_returns_input_unchanged() {
        let mut p = PassthroughMono;
        for sample in [-1.0, -0.5, 0.0, 0.0001, 0.5, 1.0] {
            assert_eq!(p.process_sample(sample), sample);
        }
    }

    #[test]
    fn passthrough_stereo_returns_input_unchanged() {
        let mut p = PassthroughStereo;
        let frame = [0.42, -0.31];
        assert_eq!(p.process_frame(frame), frame);
    }

    #[test]
    fn passthrough_mono_block_is_no_op() {
        let mut p = PassthroughMono;
        let mut buffer = vec![0.1, -0.2, 0.3, -0.4];
        let original = buffer.clone();
        p.process_block(&mut buffer);
        assert_eq!(buffer, original);
    }

    #[test]
    fn normalized_knob_returns_zero_at_zero() {
        let parameters = ts9_like_parameters();
        let ps = params_with(Some(0.0), None);
        assert_eq!(normalized_knob("level", &ps, &parameters), Some(0.0));
    }

    #[test]
    fn normalized_knob_returns_one_at_max() {
        let parameters = ts9_like_parameters();
        let ps = params_with(Some(10.0), None);
        assert_eq!(normalized_knob("level", &ps, &parameters), Some(1.0));
    }

    #[test]
    fn normalized_knob_clamps_above_max() {
        // User somehow sets level=99 in a 0–10 grid: clamp to 1.0.
        let parameters = ts9_like_parameters();
        let ps = params_with(Some(99.0), None);
        assert_eq!(normalized_knob("level", &ps, &parameters), Some(1.0));
    }
}
