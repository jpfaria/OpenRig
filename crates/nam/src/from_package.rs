//! Generic NAM instantiation from a `plugin_loader::LoadedPackage`.
//!
//! Picks the capture file that matches the user's `ParameterSet` (axes
//! declared in the manifest) and hands it to the existing
//! [`crate::build_processor_with_assets_for_layout`].
//!
//! Issues: #287 (loader) + #402 (NAM gain pedal passthrough at zero).

use anyhow::{anyhow, bail, Result};
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor, StereoProcessor};
use plugin_loader::manifest::{Backend, GridParameter};
use plugin_loader::LoadedPackage;

use crate::build_processor_with_assets_for_layout;
use crate::processor::{plugin_params_from_set_with_defaults, DEFAULT_PLUGIN_PARAMS};

/// Knob names a NAM gain pedal exposes that, when set to the lower bound
/// of the declared range, should make the block behave as a true
/// passthrough (not load or run the model). Issue #402 / #400.
const ZERO_PASSTHROUGH_KNOBS: &[&str] = &["drive", "level"];

/// Build a [`BlockProcessor`] from a disk-backed NAM package.
///
/// When a NAM gain pedal exposes a `drive` or `level` knob and the user
/// sets EITHER to the bottom of the declared range (e.g. `level: 0`),
/// returns a true passthrough — the model does not load, the block is
/// equivalent to being disabled. This is the only configuration the
/// user validated as microphonics-free for cases like `TS9 → Bogner
/// drive_red`, where the captured residual signal at "knob zero" was
/// loud enough downstream to sustain a feedback loop.
///
/// For non-zero knob values the block runs normally — `resolve_capture`
/// picks the closest grid point and the model processes input at unity.
/// No artificial dB derivation, no soft-clip, no offline normalization.
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

    if any_zero_knob(params, parameters) {
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
    // Issue #413: nivelamento de loudness é metadata estática
    // (`output_gain_db` no manifest, populado offline pelo
    // `tools/nam_loudness_audit`).
    //
    // Aplicado em DOIS pontos quando o manifest carrega o offset:
    //
    // 1. Soma em `input_level_db` — empurra signal pelo amp model.
    //    NAM responde com sua curva natural (soft saturation no top),
    //    em vez de virar gain linear pós-amp que clipa o DAC.
    //
    // 2. `audit_overrides_baked_output = true` — desliga o
    //    `recommended_output_db` baked do trainer no NamProcessor.
    //    O audit já mediu signal puro pra calibrar; manter o baked
    //    do trainer aplicado em paralelo dobra-corrige (os baked
    //    típicos atenuam -7 a -8 dB e comem todo o boost = chain
    //    sai muito quieta, foi a queixa "tudo baixo" do user).
    //
    // Usuário ainda controla o slider `input_db` (default 0, range
    // -24..+24); o offset do manifest empilha junto.
    if let Some(manifest_gain_db) = package.manifest.output_gain_db {
        plugin_params.input_level_db += manifest_gain_db;
        plugin_params.audit_overrides_baked_output = true;
    }
    build_processor_with_assets_for_layout(model_path_str, None, plugin_params, sample_rate, layout)
}

/// Returns true if any of the [`ZERO_PASSTHROUGH_KNOBS`] is declared in
/// the manifest schema AND the user set it to the lower bound (or below)
/// of the declared numeric values.
fn any_zero_knob(params: &ParameterSet, parameters: &[GridParameter]) -> bool {
    for knob in ZERO_PASSTHROUGH_KNOBS {
        let Some(user_value) = params.get_f32(knob) else {
            continue;
        };
        let Some(min_declared) = manifest_min(parameters, knob) else {
            continue;
        };
        if (user_value as f64) <= min_declared {
            return true;
        }
    }
    false
}

fn manifest_min(parameters: &[GridParameter], name: &str) -> Option<f64> {
    parameters
        .iter()
        .find(|p| p.name == name)?
        .values
        .iter()
        .filter_map(|v| match v {
            plugin_loader::manifest::ParameterValue::Number(n) => Some(*n),
            _ => None,
        })
        .fold(None, |acc: Option<f64>, n| {
            Some(acc.map_or(n, |min| min.min(n)))
        })
}

fn passthrough(layout: AudioChannelLayout) -> BlockProcessor {
    match layout {
        AudioChannelLayout::Mono => BlockProcessor::Mono(Box::new(MonoPassthrough)),
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(StereoPassthrough)),
    }
}

struct MonoPassthrough;
impl MonoProcessor for MonoPassthrough {
    fn process_sample(&mut self, input: f32) -> f32 {
        input
    }
}

struct StereoPassthrough;
impl StereoProcessor for StereoPassthrough {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        input
    }
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
