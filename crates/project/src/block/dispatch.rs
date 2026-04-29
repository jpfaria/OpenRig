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
use block_core::ModelAudioMode;
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
    match effect_type {
        EFFECT_TYPE_PREAMP => {
            validate_preamp_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_AMP => {
            validate_amp_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_FULL_RIG => {
            validate_full_rig_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_CAB => {
            validate_cab_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_BODY => {
            validate_body_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_IR => {
            validate_ir_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_GAIN => {
            validate_gain_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_WAH => {
            validate_wah_params(model, &normalized).map_err(|error| error.to_string())?
        }
        EFFECT_TYPE_PITCH => {
            validate_pitch_params(model, &normalized).map_err(|error| error.to_string())?
        }
        _ => {}
    }
    Ok(normalized)
}

pub fn schema_for_block_model(
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
        EFFECT_TYPE_MODULATION => {
            modulation_model_schema(model).map_err(|error| error.to_string())
        }
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
        EFFECT_TYPE_PREAMP | EFFECT_TYPE_AMP | EFFECT_TYPE_FULL_RIG | EFFECT_TYPE_CAB
        | EFFECT_TYPE_BODY | EFFECT_TYPE_IR | EFFECT_TYPE_GAIN | EFFECT_TYPE_DYNAMICS
        | EFFECT_TYPE_FILTER | EFFECT_TYPE_WAH | EFFECT_TYPE_PITCH | EFFECT_TYPE_MODULATION
        | EFFECT_TYPE_DELAY | EFFECT_TYPE_REVERB | EFFECT_TYPE_UTILITY => {
            AudioBlockKind::Core(CoreBlock {
                effect_type: effect_type.to_string(),
                model,
                params,
            })
        }
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
