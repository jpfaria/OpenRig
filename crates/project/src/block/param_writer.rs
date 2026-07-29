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
//! Only `Core` and `Nam` block kinds carry a `ParameterSet`; the other kinds
//! (`Input`, `Output`, `Insert`, `Select`) do not expose editable parameters
//! through these commands.

use anyhow::{anyhow, Result};
use domain::value_objects::ParameterValue;

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
    let params = params_mut(block)?;
    params.insert(path, ParameterValue::Float(value as f32));
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
/// - If the path does not exist in the block's current `ParameterSet`.
pub fn set_parameter_option(block: &mut AudioBlock, path: &str, value: &str) -> Result<()> {
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

/// Return a mutable reference to the `ParameterSet` of `block`, or an error
/// if the block kind does not carry one.
fn params_mut(block: &mut AudioBlock) -> Result<&mut block_core::param::ParameterSet> {
    match &mut block.kind {
        AudioBlockKind::Core(core) => Ok(&mut core.params),
        AudioBlockKind::Nam(nam) => Ok(&mut nam.params),
        other => Err(anyhow!(
            "block kind '{}' does not carry an editable ParameterSet",
            other.label()
        )),
    }
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "param_writer_tests.rs"]
mod tests;
