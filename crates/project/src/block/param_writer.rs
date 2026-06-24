//! Domain-level parameter writers for `AudioBlock`.
//!
//! Provides typed write operations used by `LocalDispatcher` to fulfil
//! `Command::SetBlockParameter*` variants:
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
mod tests {
    use domain::ids::BlockId;
    use domain::value_objects::ParameterValue;

    use crate::block::types::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, NamBlock};
    use crate::param::ParameterSet;

    use super::{
        set_parameter_bool, set_parameter_number, set_parameter_option, set_parameter_text,
    };

    fn make_core_block(param_path: &str, value: f32) -> AudioBlock {
        let mut params = ParameterSet::default();
        params.insert(param_path, ParameterValue::Float(value));
        AudioBlock {
            id: BlockId("blk_test".to_string()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "amp".to_string(),
                model: "test_model".to_string(),
                params,
            }),
        }
    }

    fn make_core_block_bool(param_path: &str, value: bool) -> AudioBlock {
        let mut params = ParameterSet::default();
        params.insert(param_path, ParameterValue::Bool(value));
        AudioBlock {
            id: BlockId("blk_test".to_string()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "amp".to_string(),
                model: "test_model".to_string(),
                params,
            }),
        }
    }

    fn make_core_block_string(param_path: &str, value: &str) -> AudioBlock {
        let mut params = ParameterSet::default();
        params.insert(param_path, ParameterValue::String(value.to_string()));
        AudioBlock {
            id: BlockId("blk_test".to_string()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "amp".to_string(),
                model: "test_model".to_string(),
                params,
            }),
        }
    }

    fn make_nam_block(param_path: &str, value: f32) -> AudioBlock {
        let mut params = ParameterSet::default();
        params.insert(param_path, ParameterValue::Float(value));
        AudioBlock {
            id: BlockId("blk_nam".to_string()),
            enabled: true,
            kind: AudioBlockKind::Nam(NamBlock {
                model: "some_nam".to_string(),
                params,
            }),
        }
    }

    fn make_input_block() -> AudioBlock {
        AudioBlock {
            id: BlockId("blk_input".to_string()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                io: String::new(),
                endpoint: String::new(),
            }),
        }
    }

    #[test]
    fn core_block_writes_float_value() {
        let mut block = make_core_block("gain", 0.5);
        set_parameter_number(&mut block, "gain", 0.8).expect("should succeed");
        let AudioBlockKind::Core(ref core) = block.kind else {
            panic!("expected CoreBlock");
        };
        let stored = core.params.get_f32("gain").expect("gain must be present");
        assert!((stored - 0.8_f32).abs() < 1e-5, "got {stored}");
    }

    #[test]
    fn nam_block_writes_float_value() {
        let mut block = make_nam_block("input_gain", 0.3);
        set_parameter_number(&mut block, "input_gain", 0.9).expect("should succeed");
        let AudioBlockKind::Nam(ref nam) = block.kind else {
            panic!("expected NamBlock");
        };
        let stored = nam
            .params
            .get_f32("input_gain")
            .expect("input_gain must be present");
        assert!((stored - 0.9_f32).abs() < 1e-5, "got {stored}");
    }

    #[test]
    fn missing_path_inserts_new_parameter() {
        // Issue #496: the old contract rejected unknown paths to guard
        // against typos, but it also blocked newly-exposed schema knobs
        // (output_db) from being settable on pre-existing blocks. The
        // dispatch layer only emits paths from the active schema, so
        // accepting the insert is safe and the right user behavior.
        let mut block = make_core_block("gain", 0.5);
        set_parameter_number(&mut block, "newly_exposed_knob", 0.8)
            .expect("should insert a previously-absent parameter");
        let AudioBlockKind::Core(ref core) = block.kind else {
            panic!("expected CoreBlock");
        };
        assert_eq!(core.params.get_f32("newly_exposed_knob"), Some(0.8_f32));
        assert_eq!(core.params.get_f32("gain"), Some(0.5_f32));
    }

    #[test]
    fn input_block_returns_err() {
        let mut block = make_input_block();
        let err = set_parameter_number(&mut block, "gain", 0.5)
            .expect_err("InputBlock should return Err");
        assert!(
            err.to_string().contains("ParameterSet"),
            "error must mention ParameterSet, got: {err}"
        );
    }

    // ── set_parameter_bool ────────────────────────────────────────────────────

    #[test]
    fn set_parameter_bool_writes_true() {
        let mut block = make_core_block_bool("enabled", false);
        set_parameter_bool(&mut block, "enabled", true).expect("should succeed");
        let AudioBlockKind::Core(ref core) = block.kind else {
            panic!("expected CoreBlock");
        };
        assert_eq!(core.params.get_bool("enabled"), Some(true));
    }

    #[test]
    fn set_parameter_bool_writes_false() {
        let mut block = make_core_block_bool("enabled", true);
        set_parameter_bool(&mut block, "enabled", false).expect("should succeed");
        let AudioBlockKind::Core(ref core) = block.kind else {
            panic!("expected CoreBlock");
        };
        assert_eq!(core.params.get_bool("enabled"), Some(false));
    }

    #[test]
    fn set_parameter_bool_missing_path_returns_err() {
        let mut block = make_core_block_bool("enabled", false);
        let err = set_parameter_bool(&mut block, "no_such_param", true)
            .expect_err("should fail for unknown path");
        assert!(
            err.to_string().contains("no_such_param"),
            "error must mention path, got: {err}"
        );
        // Original value unchanged
        let AudioBlockKind::Core(ref core) = block.kind else {
            panic!("expected CoreBlock");
        };
        assert_eq!(core.params.get_bool("enabled"), Some(false));
    }

    #[test]
    fn set_parameter_bool_input_block_returns_err() {
        let mut block = make_input_block();
        let err = set_parameter_bool(&mut block, "enabled", true)
            .expect_err("InputBlock should return Err");
        assert!(
            err.to_string().contains("ParameterSet"),
            "error must mention ParameterSet, got: {err}"
        );
    }

    // ── set_parameter_text ────────────────────────────────────────────────────

    #[test]
    fn set_parameter_text_writes_string() {
        let mut block = make_core_block_string("label", "old_value");
        set_parameter_text(&mut block, "label", "new_value").expect("should succeed");
        let AudioBlockKind::Core(ref core) = block.kind else {
            panic!("expected CoreBlock");
        };
        assert_eq!(core.params.get_string("label"), Some("new_value"));
    }

    #[test]
    fn set_parameter_text_missing_path_returns_err() {
        let mut block = make_core_block_string("label", "old");
        let err = set_parameter_text(&mut block, "no_such_param", "val")
            .expect_err("should fail for unknown path");
        assert!(
            err.to_string().contains("no_such_param"),
            "error must mention path, got: {err}"
        );
        // Original value unchanged
        let AudioBlockKind::Core(ref core) = block.kind else {
            panic!("expected CoreBlock");
        };
        assert_eq!(core.params.get_string("label"), Some("old"));
    }

    #[test]
    fn set_parameter_text_input_block_returns_err() {
        let mut block = make_input_block();
        let err = set_parameter_text(&mut block, "label", "val")
            .expect_err("InputBlock should return Err");
        assert!(
            err.to_string().contains("ParameterSet"),
            "error must mention ParameterSet, got: {err}"
        );
    }

    // ── set_parameter_option ──────────────────────────────────────────────────

    #[test]
    fn set_parameter_option_writes_string_value() {
        let mut block = make_core_block_string("mode", "option_a");
        set_parameter_option(&mut block, "mode", "option_b").expect("should succeed");
        let AudioBlockKind::Core(ref core) = block.kind else {
            panic!("expected CoreBlock");
        };
        assert_eq!(core.params.get_string("mode"), Some("option_b"));
    }

    #[test]
    fn set_parameter_option_missing_path_returns_err() {
        let mut block = make_core_block_string("mode", "option_a");
        let err = set_parameter_option(&mut block, "no_such_param", "option_b")
            .expect_err("should fail for unknown path");
        assert!(
            err.to_string().contains("no_such_param"),
            "error must mention path, got: {err}"
        );
        // Original value unchanged
        let AudioBlockKind::Core(ref core) = block.kind else {
            panic!("expected CoreBlock");
        };
        assert_eq!(core.params.get_string("mode"), Some("option_a"));
    }

    #[test]
    fn set_parameter_option_input_block_returns_err() {
        let mut block = make_input_block();
        let err = set_parameter_option(&mut block, "mode", "option_a")
            .expect_err("InputBlock should return Err");
        assert!(
            err.to_string().contains("ParameterSet"),
            "error must mention ParameterSet, got: {err}"
        );
    }
}
