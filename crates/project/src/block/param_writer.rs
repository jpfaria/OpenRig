//! Domain-level numeric parameter writer for `AudioBlock`.
//!
//! Provides `set_parameter_number` — the single path-indexed write operation
//! used by `LocalDispatcher` to fulfil `Command::SetBlockParameterNumber`.
//!
//! Only `Core` and `Nam` block kinds carry a `ParameterSet`; the other kinds
//! (`Input`, `Output`, `Insert`, `Select`) do not expose editable numeric
//! parameters through this command.

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
    let params = params_mut(block)?;
    if !params.values.contains_key(path) {
        return Err(anyhow!(
            "parameter '{}' not found in block '{}'",
            path,
            block.id.0
        ));
    }
    params.insert(path, ParameterValue::Float(value as f32));
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
    use domain::ids::{BlockId, DeviceId};
    use domain::value_objects::ParameterValue;

    use crate::block::types::{
        AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, NamBlock,
    };
    use crate::chain::ChainInputMode;
    use crate::param::ParameterSet;

    use super::set_parameter_number;

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
                entries: vec![InputEntry {
                    device_id: DeviceId("mic".to_string()),
                    mode: ChainInputMode::default(),
                    channels: vec![0],
                }],
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
    fn missing_path_returns_err_mentioning_path() {
        let mut block = make_core_block("gain", 0.5);
        let err = set_parameter_number(&mut block, "no_such_param", 0.8)
            .expect_err("should fail for unknown path");
        assert!(
            err.to_string().contains("no_such_param"),
            "error must mention path, got: {err}"
        );
        // Original value unchanged
        let AudioBlockKind::Core(ref core) = block.kind else {
            panic!("expected CoreBlock");
        };
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
}
