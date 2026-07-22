//! Loading/parsing helpers for audio blocks from YAML (issue #792 split).
//! Kept out of block_yaml.rs (which holds the AudioBlockYaml types + impl).

use anyhow::Result;
use domain::ids::ChainId;
use project::block::{normalize_block_params, AudioBlock};
use project::param::ParameterSet;
use serde_yaml::Value;

use crate::block_yaml::AudioBlockYaml;
use crate::flatten_parameter_set;
pub(crate) fn load_audio_block_value(
    value: Value,
    chain_id: &ChainId,
    index: usize,
) -> Option<AudioBlock> {
    let yaml = match serde_yaml::from_value::<AudioBlockYaml>(value) {
        Ok(yaml) => yaml,
        Err(error) => {
            log::warn!(
                "ignoring unsupported or invalid block at {}:{}: {}",
                chain_id.0,
                index,
                error
            );
            eprintln!(
                "ignoring unsupported or invalid block at {}:{}: {}",
                chain_id.0, index, error
            );
            return None;
        }
    };

    match yaml.into_audio_block(chain_id, index) {
        Ok(block) => {
            log::debug!("loaded block at {}:{}", chain_id.0, index);
            Some(block)
        }
        Err(error) => {
            log::warn!(
                "ignoring unsupported or invalid block at {}:{}: {}",
                chain_id.0,
                index,
                error
            );
            eprintln!(
                "ignoring unsupported or invalid block at {}:{}: {}",
                chain_id.0, index, error
            );
            None
        }
    }
}

pub(crate) fn load_model_params(
    effect_type: &str,
    model: &str,
    raw_params: Value,
) -> Result<ParameterSet> {
    let flattened = flatten_parameter_set(raw_params)?;
    normalize_block_params(effect_type, model, flattened).map_err(anyhow::Error::msg)
}

/// Migrate legacy model identifiers to their current names.
///
/// Issue #303: `native_guitar_eq` was an HPF+LPF cleanup filter; the name
/// is now occupied by the real 4-band tone-shaper EQ, and the original
/// utility moved to `native_guitar_hpf_lpf`. Legacy projects keep working
/// because we detect the legacy parameter shape (`low_cut` / `high_cut`)
/// and remap silently. New projects use whichever id they declared.
pub(crate) fn migrate_legacy_model_id(
    effect_type: &'static str,
    model: String,
    params: &Value,
) -> String {
    if effect_type == block_core::EFFECT_TYPE_FILTER && model == "native_guitar_eq" {
        let has_legacy_param = params
            .as_mapping()
            .map(|m| {
                m.contains_key(Value::String("low_cut".into()))
                    || m.contains_key(Value::String("high_cut".into()))
            })
            .unwrap_or(false);
        if has_legacy_param {
            return "native_guitar_hpf_lpf".to_string();
        }
    }
    model
}

pub(crate) fn extract_core_block_fields(
    yaml: AudioBlockYaml,
) -> (&'static str, bool, String, Value) {
    match yaml {
        AudioBlockYaml::Preamp {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_PREAMP, enabled, model, params),
        AudioBlockYaml::Amp {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_AMP, enabled, model, params),
        AudioBlockYaml::FullRig {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_FULL_RIG, enabled, model, params),
        AudioBlockYaml::Cab {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_CAB, enabled, model, params),
        AudioBlockYaml::Body {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_BODY, enabled, model, params),
        AudioBlockYaml::Ir {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_IR, enabled, model, params),
        AudioBlockYaml::Gain {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_GAIN, enabled, model, params),
        AudioBlockYaml::Delay {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_DELAY, enabled, model, params),
        AudioBlockYaml::Reverb {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_REVERB, enabled, model, params),
        AudioBlockYaml::Utility {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_UTILITY, enabled, model, params),
        AudioBlockYaml::Dynamics {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_DYNAMICS, enabled, model, params),
        AudioBlockYaml::Filter {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_FILTER, enabled, model, params),
        AudioBlockYaml::Wah {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_WAH, enabled, model, params),
        AudioBlockYaml::Modulation {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_MODULATION, enabled, model, params),
        AudioBlockYaml::Pitch {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_PITCH, enabled, model, params),
        AudioBlockYaml::Vst3 {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_VST3, enabled, model, params),
        AudioBlockYaml::Nam {
            enabled,
            model,
            params,
        } => (block_core::EFFECT_TYPE_NAM, enabled, model, params),
        AudioBlockYaml::Select { .. } => {
            unreachable!("Select handled before extract_core_block_fields")
        }
        AudioBlockYaml::Input { .. } => {
            unreachable!("Input handled before extract_core_block_fields")
        }
        AudioBlockYaml::Output { .. } => {
            unreachable!("Output handled before extract_core_block_fields")
        }
        AudioBlockYaml::Insert { .. } => {
            unreachable!("Insert handled before extract_core_block_fields")
        }
    }
}
