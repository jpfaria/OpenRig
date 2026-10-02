//! Responsibility: writes one parameter value into a block.
//! Domain-level parameter writers for `AudioBlock`.
//!
//! Provides typed write operations used by `LocalDispatcher` to fulfil
//! `BlockCommand::SetBlockParameter*` variants:
//! - `set_parameter_number` — f64 value → `ParameterValue::Float`
//! - `set_parameter_bool`   — bool value → `ParameterValue::Bool`
//! - `set_parameter_text`   — string value → `ParameterValue::String`
//! - `set_parameter_option` — string option value → `ParameterValue::String`
//! - `set_parameter_file`   — file path (as string) → `ParameterValue::String`
//!
//! Only `Core`, `Nam` and `Split` (#328: the split and mixer knobs) carry a
//! `ParameterSet` (see [`super::block_params`]); `Input`, `Output`, `Insert`
//! and `Select` expose no editable parameters through these commands.

use anyhow::{anyhow, Result};
use domain::value_objects::ParameterValue;

use super::block_params::block_params_mut;
use super::dispatch::schema_for_block_model;
use super::split_params::check_split_knob;
use super::types::{AudioBlock, AudioBlockKind};

/// Write `value` (f64 → stored as `ParameterValue::Float`) to the parameter
/// identified by `path` inside `block`.
///
/// # Errors
///
/// - If the block kind does not carry a `ParameterSet` (Input, Output, Insert,
///   Select).
/// - If the path does not exist in the block's current `ParameterSet`.
pub fn set_parameter_number(block: &mut AudioBlock, path: &str, value: f64) -> Result<()> {
    // Issue #496: removed the `contains_key` guard. A NAM block saved
    // before #496 (when `output_db` was filtered out of the schema)
    // has no `output_db` entry in its ParameterSet — the old guard
    // rejected the first attempt to set it, so the GUI knob kept
    // reverting to default. The Command/dispatch layer already only
    // emits paths drawn from the active schema (see
    // `block_parameter_items_for_model`), so accepting an insert here
    // is safe; rejection just enforced "must have been written before"
    // which prevents introducing newly-exposed parameters.
    let value = ParameterValue::Float(value as f32);
    refuse_invalid_split_knob(block, path, &value)?;
    let params = params_mut(block)?;
    params.insert(path, value);
    Ok(())
}

/// Write `value` as `ParameterValue::Bool` to the parameter identified by
/// `path` inside `block`.
///
/// # Errors
///
/// - If the block kind does not carry a `ParameterSet`.
/// - If the path does not exist in the block's current `ParameterSet`.
pub fn set_parameter_bool(block: &mut AudioBlock, path: &str, value: bool) -> Result<()> {
    let params = params_mut(block)?;
    if !params.values.contains_key(path) {
        return Err(anyhow!(
            "parameter '{}' not found in block '{}'",
            path,
            block.id.0
        ));
    }
    params.insert(path, ParameterValue::Bool(value));
    Ok(())
}

/// Write `value` as `ParameterValue::String` to the parameter identified by
/// `path` inside `block`.
///
/// Used by both `SetBlockParameterText` and `PickBlockParameterFile` (the
/// latter resolves the path to a string in the adapter before dispatching).
///
/// # Errors
///
/// - If the block kind does not carry a `ParameterSet`.
/// - If the path does not exist in the block's current `ParameterSet`.
pub fn set_parameter_text(block: &mut AudioBlock, path: &str, value: &str) -> Result<()> {
    let params = params_mut(block)?;
    if !params.values.contains_key(path) {
        return Err(anyhow!(
            "parameter '{}' not found in block '{}'",
            path,
            block.id.0
        ));
    }
    params.insert(path, ParameterValue::String(value.to_string()));
    Ok(())
}

/// Write the selected option `value` (a string option key) as
/// `ParameterValue::String` to the parameter identified by `path` inside
/// `block`.
///
/// The adapter layer resolves the index → string before building the command,
/// so this function receives the canonical option string directly.
///
/// # Errors
///
/// - If the block kind does not carry a `ParameterSet`.
/// - If the path is neither in the block's current `ParameterSet` nor
///   declared by its model's schema. A block saved before an option joined
///   its schema (a delay's `time_sync`) has no entry for it yet, so a path
///   the schema declares is inserted.
pub fn set_parameter_option(block: &mut AudioBlock, path: &str, value: &str) -> Result<()> {
    refuse_invalid_split_knob(block, path, &ParameterValue::String(value.to_string()))?;
    let declared = schema_declares(block, path);
    let params = params_mut(block)?;
    if !params.values.contains_key(path) && !declared {
        return Err(anyhow!(
            "parameter '{}' not found in block '{}'",
            path,
            block.id.0
        ));
    }
    params.insert(path, ParameterValue::String(value.to_string()));
    Ok(())
}

/// #328: a split knob is checked against the split schema before it is stored.
fn refuse_invalid_split_knob(block: &AudioBlock, path: &str, value: &ParameterValue) -> Result<()> {
    let AudioBlockKind::Split(split) = &block.kind else {
        return Ok(());
    };
    check_split_knob(path, value.clone(), split.paths.len())
        .map_err(|e| anyhow!("invalid value for split '{}': {e}", block.id.0))
}

/// Whether the schema of a core block's model declares `path`.
fn schema_declares(block: &AudioBlock, path: &str) -> bool {
    let AudioBlockKind::Core(core) = &block.kind else {
        return false;
    };
    schema_for_block_model(&core.effect_type, &core.model)
        .is_ok_and(|schema| schema.parameters.iter().any(|spec| spec.path == path))
}

/// Return a mutable reference to the `ParameterSet` of `block`, or an error
/// if the block kind does not carry one.
fn params_mut(block: &mut AudioBlock) -> Result<&mut block_core::param::ParameterSet> {
    let label = block.kind.label();
    block_params_mut(&mut block.kind).ok_or_else(|| {
        anyhow!(
            "block kind '{}' does not carry an editable ParameterSet",
            label
        )
    })
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "param_writer_tests.rs"]
mod tests;
