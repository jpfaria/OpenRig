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

use domain::ids::BlockId;
use project::block::{
    build_audio_block_kind, schema_for_block_model, AudioBlock, AudioBlockKind, InsertBlock,
};
use project::param::ParameterSet;

/// Build a default [`AudioBlock`] for the given (effect_type, model_id) pair.
///
/// All parameters are reset to the schema's default values. The block starts
/// `enabled = true`. The caller supplies the `block_id` to use.
///
/// Special case: `effect_type == "insert"` produces a default `InsertBlock`
/// that is unbound (`io` empty) — the user picks its I/O binding afterwards
/// (#716, model A). `model_id` is ignored for insert blocks (always
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
        // Model A (#716): a fresh insert is unbound — the user picks its I/O
        // binding (`io`) later; the send/return device endpoints are then
        // resolved from the per-machine registry.
        return Ok(AudioBlock {
            id: block_id,
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".to_string(),
                io: String::new(),
            }),
        });
    }
    let params = default_params_for_model(effect_type, model_id)?;
    let kind = build_audio_block_kind(effect_type, model_id, params)
        .map_err(|e| anyhow!("build_audio_block_kind failed for '{}': {}", model_id, e))?;
    Ok(AudioBlock {
        id: block_id,
        enabled: true,
        kind,
    })
}

/// Build the seeded default [`ParameterSet`] for a new `(effect_type,
/// model_id)` block — the single source of param seeding shared by
/// [`build_default_block`] (the `AddBlock` path) and the GUI block editor
/// (which builds the `InsertPrebuiltBlock` it dispatches). Every knob starts
/// at its schema default; then the grid axes are seeded to the first declared
/// capture (#630), and the `output_db` (#655) and `noise_gate` (#675) knobs
/// are seeded from the manifest so the user-visible knobs arrive
/// pre-configured (editable, persisted) — never a hidden load-time default.
pub fn default_params_for_model(effect_type: &str, model_id: &str) -> Result<ParameterSet> {
    let schema = schema_for_block_model(effect_type, model_id).map_err(|e| {
        anyhow!(
            "unknown model '{}' for effect type '{}': {}",
            model_id,
            effect_type,
            e
        )
    })?;
    // Issue #630: a grid pedal (NAM/IR capture grid) must be born at a REAL
    // capture, never at the per-axis-minimum combination. `normalized_against`
    // fills each axis independently with its first declared value, which for a
    // multi-axis grid can yield a cell that does NOT exist. Seed the FIRST
    // declared capture's axis values up front so the born default is a
    // deterministic, audible grid point.
    let mut seed = ParameterSet::default();
    if let Some(pkg) = plugin_loader::registry::find(model_id) {
        let grid = match &pkg.manifest.backend {
            plugin_loader::manifest::Backend::Nam {
                parameters,
                captures,
            }
            | plugin_loader::manifest::Backend::Ir {
                parameters,
                captures,
            } => plugin_loader::dispatch::first_capture_axis_values(parameters, captures),
            _ => Vec::new(),
        };
        for (name, value) in grid {
            seed.insert(name, manifest_value_to_param(value));
        }
    }
    let mut params = seed
        .normalized_against(&schema)
        .map_err(|e| anyhow!("param normalisation failed for '{}': {}", model_id, e))?;
    // Seed `output_db` and `noise_gate` from the manifest so the user-visible
    // knobs mirror the plugin's calibration from day one. Applying these
    // silently at load time instead would hide them under knobs reading the
    // schema default (the bug #655 fixed for `output_db`, #675 for the gate).
    if let Some(pkg) = plugin_loader::registry::find(model_id) {
        if let Some(audit_db) = pkg.manifest.output_gain_db {
            params.insert(
                "output_db",
                domain::value_objects::ParameterValue::Float(audit_db),
            );
        }
        let capture_gate = match &pkg.manifest.backend {
            plugin_loader::manifest::Backend::Nam {
                parameters,
                captures,
            } => plugin_loader::dispatch::resolve_capture(parameters, captures, &params)
                .and_then(|c| c.noise_gate.as_ref()),
            _ => None,
        };
        let (gate_enabled, gate_threshold_db) = plugin_loader::manifest::resolve_noise_gate(
            capture_gate,
            pkg.manifest.noise_gate.as_ref(),
        );
        if let Some(enabled) = gate_enabled {
            params.insert(
                "noise_gate.enabled",
                domain::value_objects::ParameterValue::Bool(enabled),
            );
        }
        if let Some(threshold_db) = gate_threshold_db {
            params.insert(
                "noise_gate.threshold_db",
                domain::value_objects::ParameterValue::Float(threshold_db),
            );
        }
    }
    Ok(params)
}

/// Convert a manifest grid `ParameterValue` into the block `ParameterValue`
/// stored in a `ParameterSet`. Numeric axes become `Float` (the grid stores
/// `f64`; the runtime resolves captures with `get_f32`), text axes become
/// `String`, bool axes become `Bool`.
fn manifest_value_to_param(
    value: plugin_loader::manifest::ParameterValue,
) -> domain::value_objects::ParameterValue {
    use domain::value_objects::ParameterValue as Param;
    use plugin_loader::manifest::ParameterValue as Manifest;
    match value {
        Manifest::Number(n) => Param::Float(n as f32),
        Manifest::Text(t) => Param::String(t),
        Manifest::Bool(b) => Param::Bool(b),
    }
}

/// Determine which effect_type owns the given model_id.
///
/// Disk-package models (NAM, IR, LV2, VST3) declare a single `type` in
/// their manifest; the `plugin_loader` registry holds it. Reading it
/// there is the authoritative resolution and is tried first.
///
/// Issue #537 — the trial loop below used to be the only path: it
/// scanned effect_types in declaration order and returned the first one
/// whose `schema_for_block_model` lookup succeeded. The disk-package
/// schema lookup does not filter by effect_type, so any cab IR
/// (`ir_v30_4x12`) matched `EFFECT_TYPE_PREAMP` first (because preamp
/// leads the list) and the slot morphed cab→preamp on swap, sending the
/// IR convolver through a preamp slot at runtime — broadband noise.
///
/// Native models still fall through to the trial loop; natives register
/// under a single effect_type per `MODEL_DEFINITION`, so the first
/// successful match is correct for them.
///
/// # Errors
///
/// Returns `Err` when the model is neither a registered disk package nor
/// resolvable through any native effect_type registry.
pub fn resolve_effect_type_for_model(model_id: &str) -> Result<String> {
    if let Some(pkg) = plugin_loader::registry::find(model_id) {
        return Ok(block_type_to_effect_type(pkg.manifest.block_type).to_string());
    }
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

/// Map a manifest's `BlockType` to the canonical effect_type string
/// defined in `block-core`. Counterpart to the private mapper in
/// `project::catalog::block_type_for_effect_type` (kept private there
/// because it's only consulted from the catalog). Lives here so this
/// crate can resolve disk-package types without growing `catalog.rs`
/// (already past the 600-line cap).
fn block_type_to_effect_type(block_type: plugin_loader::manifest::BlockType) -> &'static str {
    use block_core::*;
    use plugin_loader::manifest::BlockType;
    match block_type {
        BlockType::Preamp => EFFECT_TYPE_PREAMP,
        BlockType::Amp => EFFECT_TYPE_AMP,
        BlockType::Cab => EFFECT_TYPE_CAB,
        BlockType::Body => EFFECT_TYPE_BODY,
        BlockType::GainPedal => EFFECT_TYPE_GAIN,
        BlockType::Delay => EFFECT_TYPE_DELAY,
        BlockType::Reverb => EFFECT_TYPE_REVERB,
        BlockType::Mod => EFFECT_TYPE_MODULATION,
        BlockType::Dyn => EFFECT_TYPE_DYNAMICS,
        BlockType::Filter => EFFECT_TYPE_FILTER,
        BlockType::Wah => EFFECT_TYPE_WAH,
        BlockType::Pitch => EFFECT_TYPE_PITCH,
        BlockType::Util => EFFECT_TYPE_UTILITY,
    }
}
