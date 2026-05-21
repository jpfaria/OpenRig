//! Default-block construction for `Command::AddBlock` and `Command::ReplaceBlockModel`.
//!
//! This is business logic: given an (effect_type, model_id) pair, build a
//! default `AudioBlock` with all parameters reset to their schema defaults.
//! Lives in `crates/application` — NOT in `adapter-gui` — so every transport
//! (GUI, gRPC, MIDI, CLI) can use the same construction path.
//!
//! The `BlockId` for a new block is generated deterministically from the chain
//! and model context; callers that need a stable id can supply one via the
//! optional `block_id` parameter.

use anyhow::{anyhow, Result};

use domain::ids::{BlockId, DeviceId};
use project::block::{
    build_audio_block_kind, schema_for_block_model, AudioBlock, AudioBlockKind, InsertBlock,
    InsertEndpoint,
};
use project::chain::ChainInputMode;
use project::param::ParameterSet;

/// Build a default [`AudioBlock`] for the given (effect_type, model_id) pair.
///
/// All parameters are reset to the schema's default values. The block starts
/// `enabled = true`. The caller supplies the `block_id` to use.
///
/// Special case: `effect_type == "insert"` produces a default `InsertBlock`
/// with empty send/return endpoints (the user configures them in the insert
/// window afterwards). `model_id` is ignored for insert blocks (always
/// "standard").
///
/// # Errors
///
/// Returns `Err` when `effect_type` or `model_id` is not recognised by the
/// block registry (schema lookup fails). Insert blocks never return an error.
pub fn build_default_block(
    block_id: BlockId,
    effect_type: &str,
    model_id: &str,
) -> Result<AudioBlock> {
    if effect_type == "insert" {
        let empty_endpoint = InsertEndpoint {
            device_id: DeviceId(String::new()),
            mode: ChainInputMode::Mono,
            channels: Vec::new(),
        };
        return Ok(AudioBlock {
            id: block_id,
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".to_string(),
                send: empty_endpoint.clone(),
                return_: empty_endpoint,
            }),
        });
    }
    let schema = schema_for_block_model(effect_type, model_id).map_err(|e| {
        anyhow!(
            "unknown model '{}' for effect type '{}': {}",
            model_id,
            effect_type,
            e
        )
    })?;
    let mut params = ParameterSet::default()
        .normalized_against(&schema)
        .map_err(|e| anyhow!("param normalisation failed for '{}': {}", model_id, e))?;
    // Seed `output_db` from the plugin manifest's audit baseline so
    // the user-visible knob mirrors the engine's actual offset from
    // day one. The previous design added the audit silently at load
    // time (in `nam::from_package`) — that hid the offset under a
    // UI knob that read 0 even though the signal was being attenuated.
    if let Some(pkg) = plugin_loader::registry::find(model_id) {
        if let Some(audit_db) = pkg.manifest.output_gain_db {
            params.insert(
                "output_db",
                domain::value_objects::ParameterValue::Float(audit_db),
            );
        }
    }
    let kind = build_audio_block_kind(effect_type, model_id, params)
        .map_err(|e| anyhow!("build_audio_block_kind failed for '{}': {}", model_id, e))?;
    Ok(AudioBlock {
        id: block_id,
        enabled: true,
        kind,
    })
}

/// Determine which effect_type owns the given model_id by attempting a schema
/// lookup across the known effect_type values.
///
/// This is only needed for `ReplaceBlockModel` when we want to let the caller
/// specify just the model_id without the effect_type. In this project the
/// model_id is unique within the registry, so we can resolve it by trial.
///
/// Returns the effect_type string that resolves the model, or `Err` if none
/// does.
pub fn resolve_effect_type_for_model(model_id: &str) -> Result<String> {
    use block_core::*;
    let candidate_types = [
        EFFECT_TYPE_PREAMP,
        EFFECT_TYPE_AMP,
        EFFECT_TYPE_FULL_RIG,
        EFFECT_TYPE_CAB,
        EFFECT_TYPE_BODY,
        EFFECT_TYPE_IR,
        EFFECT_TYPE_GAIN,
        EFFECT_TYPE_NAM,
        EFFECT_TYPE_DELAY,
        EFFECT_TYPE_REVERB,
        EFFECT_TYPE_UTILITY,
        EFFECT_TYPE_DYNAMICS,
        EFFECT_TYPE_FILTER,
        EFFECT_TYPE_WAH,
        EFFECT_TYPE_PITCH,
        EFFECT_TYPE_MODULATION,
        EFFECT_TYPE_VST3,
    ];
    for et in &candidate_types {
        if schema_for_block_model(et, model_id).is_ok() {
            return Ok((*et).to_string());
        }
    }
    Err(anyhow!(
        "model_id '{}' not found in any known effect_type registry",
        model_id
    ))
}
