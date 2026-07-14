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
        // Streams are ALWAYS stereo internally (CLAUDE.md invariant #5).
        // Mono-native plugins (NAM, IR, LV2 1in/1out) run as DualMono:
        // one instance per channel. Forcing MonoOnly here would make
        // the engine downmix to mono, violating the stereo invariant.
        audio_mode: block_core::ModelAudioMode::DualMono,
        parameters,
    })
}

/// Build a `Vec<ParameterSpec>` from a `LoadedPackage` manifest. One
/// module per backend does the actual work:
/// - NAM (`nam_schema`): capture axes (Capture tab) + engine defaults (Amp tab).
/// - IR (`ir_schema`): capture axes + Output / reverb controls, one flat grid.
/// - LV2 (`lv2_schema`): one control per `ControlIn` port scanned off the TTL.
/// - VST3: each declared `Vst3Parameter` becomes a float param.
/// - Native: nothing — natives go through the legacy schema path so
///   this branch shouldn't fire in practice; return empty to avoid a
///   panic if it ever does.
pub(crate) fn synthesize_parameters_from_manifest(
    package: &plugin_loader::LoadedPackage,
) -> Vec<block_core::param::ParameterSpec> {
    use plugin_loader::manifest::Backend;
    match &package.manifest.backend {
        Backend::Nam {
            parameters,
            captures,
        } => super::nam_schema::nam_parameters(package, parameters, captures),
        Backend::Ir {
            parameters,
            captures,
        } => super::ir_schema::ir_parameters(package, parameters, captures),
        Backend::Lv2 {
            plugin_uri,
            binaries,
        } => super::lv2_schema::lv2_parameters(package, plugin_uri, binaries),
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
            // #780: light scan leaves entry.info.params empty; synthesise the
            // schema from the plugin's real parameters (knob / toggle / select
            // per step_count). See `block::vst3_schema`.
            let parameters = crate::block::vst3_schema::vst3_parameters(model);
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
    let ctx = block_core::param::MaterializeContext {
        block_id,
        effect_type,
        model,
        audio_mode: schema.audio_mode,
    };
    Ok(schema
        .parameters
        .iter()
        .map(|spec| {
            let current_value = normalized
                .get(&spec.path)
                .cloned()
                .or_else(|| spec.default_value.clone())
                .unwrap_or(ParameterValue::Null);
            spec.materialize(&ctx, current_value)
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
