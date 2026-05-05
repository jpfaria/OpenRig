//! Per-effect-type dispatch — the three big match arms that route a
//! block (`effect_type`, `model`) to the right `block-*` crate's schema /
//! validation / kind constructor, plus two private describe helpers.
//!
//! Lifted out of `block.rs` (Phase 7 of issue #194). One responsibility:
//! cross-crate dispatch on `effect_type`. The acceptance criterion's
//! second half ("dispatch via trait") is a follow-up that would replace
//! the match chains with a registry-backed trait — out of scope for the
//! file split.

use block_amp::{amp_model_schema, validate_amp_params};
use block_body::{body_model_schema, validate_body_params};
use block_cab::{cab_model_schema, validate_cab_params};
use block_delay::delay_model_schema;
use block_dyn::dynamics_model_schema;
use block_filter::filter_model_schema;
use block_full_rig::{full_rig_model_schema, validate_full_rig_params};
use block_gain::{gain_model_schema, validate_gain_params};
use block_ir::{ir_model_schema, validate_ir_params};
use block_mod::modulation_model_schema;
use block_nam::nam_model_schema;
use block_pitch::{pitch_model_schema, validate_pitch_params};
use block_preamp::{preamp_model_schema, validate_preamp_params};
use block_reverb::reverb_model_schema;
use block_util::utility_model_schema;
use block_wah::{validate_wah_params, wah_model_schema};
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;

use crate::param::{BlockParameterDescriptor, ModelParameterSchema, ParameterSet};

use super::types::{AudioBlockKind, BlockAudioDescriptor, CoreBlock, NamBlock};

pub fn normalize_block_params(
    effect_type: &str,
    model: &str,
    params: ParameterSet,
) -> Result<ParameterSet, String> {
    let schema = schema_for_block_model(effect_type, model)?;
    let normalized = params.normalized_against(&schema)?;
    use block_core::*;
    let result: Result<(), anyhow::Error> = match effect_type {
        EFFECT_TYPE_PREAMP => validate_preamp_params(model, &normalized),
        EFFECT_TYPE_AMP => validate_amp_params(model, &normalized),
        EFFECT_TYPE_FULL_RIG => validate_full_rig_params(model, &normalized),
        EFFECT_TYPE_CAB => validate_cab_params(model, &normalized),
        EFFECT_TYPE_BODY => validate_body_params(model, &normalized),
        EFFECT_TYPE_IR => validate_ir_params(model, &normalized),
        EFFECT_TYPE_GAIN => validate_gain_params(model, &normalized),
        EFFECT_TYPE_WAH => validate_wah_params(model, &normalized),
        EFFECT_TYPE_PITCH => validate_pitch_params(model, &normalized),
        _ => Ok(()),
    };
    if let Err(error) = result {
        // Disk packages don't live in the legacy block-* registry that
        // validate_*_params consults, so an Err here is expected for
        // disk-backed models — accept them and let the package builder
        // surface any real param mismatch at instantiation time. Issue #287.
        if plugin_loader::registry::find(model).is_none() {
            return Err(error.to_string());
        }
    }
    Ok(normalized)
}

pub fn schema_for_block_model(
    effect_type: &str,
    model: &str,
) -> Result<ModelParameterSchema, String> {
    schema_for_block_model_legacy(effect_type, model)
        .or_else(|legacy_err| schema_from_disk_package(effect_type, model, legacy_err))
}

/// Fallback: if the legacy block-* registry doesn't recognise the model,
/// try `plugin_loader::registry`. Synthesizes a parameter schema from
/// the manifest data so the GUI can render knobs for disk-backed
/// plugins. Issue: #287.
fn schema_from_disk_package(
    effect_type: &str,
    model: &str,
    legacy_err: String,
) -> Result<ModelParameterSchema, String> {
    let package = plugin_loader::registry::find(model).ok_or(legacy_err)?;
    let parameters = synthesize_parameters_from_manifest(package);
    Ok(ModelParameterSchema {
        effect_type: effect_type.to_string(),
        model: package.manifest.id.clone(),
        display_name: package.manifest.display_name.clone(),
        audio_mode: audio_mode_for_backend(package),
        parameters,
    })
}

/// Pick the right `ModelAudioMode` for a disk-backed package's DSP
/// topology. Hardcoding `DualMono` for every backend caused NAM amps
/// (mono-only) to be instantiated twice (left + right) and the NAM
/// C SDK couldn't host two concurrent instances of the same model
/// without producing runtime feedback (issue #287, "microfonia ao
/// ativar mesa rectifier").
fn audio_mode_for_backend(package: &plugin_loader::LoadedPackage) -> block_core::ModelAudioMode {
    use block_core::ModelAudioMode;
    use plugin_loader::manifest::Backend;
    match &package.manifest.backend {
        // NAM and IR are mono-native: one DSP instance, broadcast to
        // stereo at the engine layer.
        Backend::Nam { .. } | Backend::Ir { .. } => ModelAudioMode::MonoOnly,
        // LV2 audio shape is decided per-bundle by counting AudioIn /
        // AudioOut ports. We keep DualMono here as the safe default —
        // most LV2 effects work fine with twin mono runs — but mono
        // amp-style LV2s (1 in, 1 out) should ideally surface as
        // MonoOnly. For now this matches pre-#287 LV2 behavior.
        Backend::Lv2 { .. } => ModelAudioMode::DualMono,
        // VST3 plugins are typically true stereo.
        Backend::Vst3 { .. } => ModelAudioMode::TrueStereo,
        Backend::Native { .. } => ModelAudioMode::DualMono,
    }
}

/// Build a `Vec<ParameterSpec>` from a `LoadedPackage` manifest:
/// - NAM/IR: each grid axis becomes a float (numeric values) or enum
///   (text values) parameter spanning the declared min..max.
/// - LV2: scan the bundle's TTL files and emit a float param per
///   `ControlIn` port using its TTL min/max/default.
/// - VST3: each declared `Vst3Parameter` becomes a 0..1 float param.
/// - Native: nothing — natives go through the legacy schema path so
///   this branch shouldn't fire in practice; return empty to avoid a
///   panic if it ever does.
fn synthesize_parameters_from_manifest(
    package: &plugin_loader::LoadedPackage,
) -> Vec<block_core::param::ParameterSpec> {
    use plugin_loader::manifest::Backend;
    match &package.manifest.backend {
        Backend::Nam { parameters, .. } | Backend::Ir { parameters, .. } => {
            parameters.iter().map(grid_parameter_to_spec).collect()
        }
        Backend::Lv2 {
            plugin_uri,
            binaries,
        } => synthesize_lv2_parameters(package, plugin_uri, binaries),
        Backend::Vst3 { parameters, .. } => parameters
            .iter()
            .map(|param| {
                block_core::param::float_parameter(
                    &param.name,
                    param.display_name.as_deref().unwrap_or(&param.name),
                    None,
                    Some(param.default as f32),
                    param.min as f32,
                    param.max as f32,
                    param.step.unwrap_or(0.01) as f32,
                    block_core::param::ParameterUnit::None,
                )
            })
            .collect(),
        Backend::Native { .. } => Vec::new(),
    }
}

fn grid_parameter_to_spec(
    parameter: &plugin_loader::manifest::GridParameter,
) -> block_core::param::ParameterSpec {
    use plugin_loader::manifest::ParameterValue;
    let label = parameter.display_name.as_deref().unwrap_or(&parameter.name);
    let all_numeric = parameter
        .values
        .iter()
        .all(|v| matches!(v, ParameterValue::Number(_)));
    if all_numeric && !parameter.values.is_empty() {
        let numbers: Vec<f64> = parameter
            .values
            .iter()
            .filter_map(|v| match v {
                ParameterValue::Number(n) => Some(*n),
                _ => None,
            })
            .collect();
        let min = numbers.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let default = numbers.first().copied().unwrap_or(min);
        let step = if numbers.len() > 1 {
            (numbers[1] - numbers[0]).abs() as f32
        } else {
            1.0_f32
        };
        block_core::param::float_parameter(
            &parameter.name,
            label,
            None,
            Some(default as f32),
            min as f32,
            max as f32,
            step.max(0.01),
            block_core::param::ParameterUnit::None,
        )
    } else {
        let options: Vec<(String, String)> = parameter
            .values
            .iter()
            .map(|value| {
                let s = match value {
                    ParameterValue::Text(t) => t.clone(),
                    ParameterValue::Number(n) => n.to_string(),
                };
                (s.clone(), s)
            })
            .collect();
        let option_refs: Vec<(&str, &str)> = options
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let default = options.first().map(|(k, _)| k.as_str());
        block_core::param::enum_parameter(&parameter.name, label, None, default, &option_refs)
    }
}

fn synthesize_lv2_parameters(
    package: &plugin_loader::LoadedPackage,
    plugin_uri: &str,
    binaries: &std::collections::BTreeMap<plugin_loader::manifest::Lv2Slot, std::path::PathBuf>,
) -> Vec<block_core::param::ParameterSpec> {
    use plugin_loader::dispatch::Lv2PortRole;
    // Prefer the deduplicated `<package>/data/` TTL bundle; fall back
    // to the legacy per-platform layout where TTLs lived next to the
    // binary. Either layout works.
    let data_dir = package.root.join("data");
    let bundle_dir: std::path::PathBuf = if data_dir.is_dir() {
        data_dir
    } else if let Some((_, rel_binary)) = binaries.iter().next() {
        let bin_path = package.root.join(rel_binary);
        match bin_path.parent() {
            Some(parent) => parent.to_path_buf(),
            None => return Vec::new(),
        }
    } else {
        return Vec::new();
    };
    let Ok(ports) = plugin_loader::dispatch::scan_lv2_ports(&bundle_dir, plugin_uri) else {
        return Vec::new();
    };
    ports
        .into_iter()
        .filter(|port| port.role == Lv2PortRole::ControlIn)
        .map(|port| {
            let label = port.name.clone().unwrap_or_else(|| port.symbol.clone());
            let min = port.minimum.unwrap_or(0.0);
            let max = port.maximum.unwrap_or(1.0).max(min + 0.001);
            let default = port.default_value.unwrap_or((min + max) / 2.0);
            // step = 0 means "continuous" (no snap-to-grid). LV2
            // ControlPorts are continuous unless the TTL marks them
            // `lv2:portProperty lv2:integer` or `lv2:enumeration` —
            // we don't parse those flags yet, and in any case the
            // synthesized step was `(max-min)/100` which generated
            // bogus grids (e.g. Contour 20-20000 → step 199.8) that
            // rejected the TTL default itself.
            block_core::param::float_parameter(
                &port.symbol,
                &label,
                None,
                Some(default),
                min,
                max,
                0.0,
                block_core::param::ParameterUnit::None,
            )
        })
        .collect()
}

fn schema_for_block_model_legacy(
    effect_type: &str,
    model: &str,
) -> Result<ModelParameterSchema, String> {
    use block_core::*;
    match effect_type {
        EFFECT_TYPE_PREAMP => preamp_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_AMP => amp_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_FULL_RIG => full_rig_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_CAB => cab_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_BODY => body_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_IR => ir_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_GAIN => gain_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_NAM => nam_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_DELAY => delay_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_REVERB => reverb_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_UTILITY => utility_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_DYNAMICS => dynamics_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_FILTER => filter_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_WAH => wah_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_PITCH => pitch_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_MODULATION => modulation_model_schema(model).map_err(|error| error.to_string()),
        x if x == block_core::EFFECT_TYPE_VST3 => {
            let entry = vst3_host::find_vst3_plugin(model)
                .ok_or_else(|| format!("VST3 plugin '{}' not found in catalog", model))?;
            // Build a float parameter for each discovered VST3 parameter (normalized 0–100%).
            let parameters = entry
                .info
                .params
                .iter()
                .map(|p| {
                    let path = format!("p{}", p.id);
                    let label = if p.title.is_empty() {
                        p.short_title.clone()
                    } else {
                        p.title.clone()
                    };
                    let default_pct = (p.default_normalized * 100.0) as f32;
                    block_core::param::float_parameter(
                        &path,
                        &label,
                        None,
                        Some(default_pct),
                        0.0,
                        100.0,
                        1.0,
                        block_core::param::ParameterUnit::Percent,
                    )
                })
                .collect();
            Ok(ModelParameterSchema {
                effect_type: block_core::EFFECT_TYPE_VST3.to_string(),
                model: model.to_string(),
                display_name: entry.display_name.to_string(),
                audio_mode: ModelAudioMode::MonoToStereo,
                parameters,
            })
        }
        other => Err(format!("unsupported block type '{}'", other)),
    }
}

pub fn build_audio_block_kind(
    effect_type: &str,
    model: &str,
    params: ParameterSet,
) -> Result<AudioBlockKind, String> {
    let model = model.to_string();
    use block_core::*;
    let kind = match effect_type {
        EFFECT_TYPE_PREAMP
        | EFFECT_TYPE_AMP
        | EFFECT_TYPE_FULL_RIG
        | EFFECT_TYPE_CAB
        | EFFECT_TYPE_BODY
        | EFFECT_TYPE_IR
        | EFFECT_TYPE_GAIN
        | EFFECT_TYPE_DYNAMICS
        | EFFECT_TYPE_FILTER
        | EFFECT_TYPE_WAH
        | EFFECT_TYPE_PITCH
        | EFFECT_TYPE_MODULATION
        | EFFECT_TYPE_DELAY
        | EFFECT_TYPE_REVERB
        | EFFECT_TYPE_UTILITY => AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.to_string(),
            model,
            params,
        }),
        EFFECT_TYPE_NAM => AudioBlockKind::Nam(NamBlock { model, params }),
        x if x == EFFECT_TYPE_VST3 => AudioBlockKind::Core(CoreBlock {
            effect_type: EFFECT_TYPE_VST3.to_string(),
            model,
            params,
        }),
        other => return Err(format!("unsupported block type '{}'", other)),
    };
    Ok(kind)
}

pub(super) fn describe_block_params(
    block_id: &BlockId,
    effect_type: &str,
    model: &str,
    params: &ParameterSet,
) -> Result<Vec<BlockParameterDescriptor>, String> {
    let schema = schema_for_block_model(effect_type, model)?;
    let normalized = params.normalized_against(&schema)?;
    Ok(schema
        .parameters
        .iter()
        .map(|spec| {
            let current_value = normalized
                .get(&spec.path)
                .cloned()
                .or_else(|| spec.default_value.clone())
                .unwrap_or(ParameterValue::Null);
            spec.materialize(
                block_id,
                effect_type,
                model,
                schema.audio_mode,
                current_value,
            )
        })
        .collect())
}

pub(super) fn describe_block_audio(
    block_id: &BlockId,
    effect_type: &str,
    model: &str,
) -> Result<BlockAudioDescriptor, String> {
    let schema = schema_for_block_model(effect_type, model)?;
    Ok(BlockAudioDescriptor {
        block_id: block_id.clone(),
        effect_type: effect_type.to_string(),
        model: schema.model,
        display_name: schema.display_name,
        audio_mode: schema.audio_mode,
    })
}
