//! Issue #655: re-seed the IR `output_db` knob when the user changes a
//! capture axis.
//!
//! The IR Output Level knob is an *absolute* dB value (mirroring NAM):
//! its default mirrors the selected capture's audit baseline. When the
//! user picks a different mic/position the resolved capture changes, so
//! the knob must re-seed to the new capture's baseline — otherwise the
//! previously selected capture's offset would be applied to the new IR,
//! changing its loudness (volume invariant #10).

use plugin_loader::manifest::{Backend, GridCapture, GridParameter};
use project::block::AudioBlock;
use project::param::ParameterSet;

/// The `output_db` (absolute dB) the IR knob should re-seed to after a
/// capture-axis change: the newly selected capture's audit baseline, with
/// the manifest-level value as fallback. Returns `None` when the changed
/// param is `output_db` itself (the user is dragging the knob — never
/// clobber that) or when no capture/audit resolves; callers then leave the
/// knob untouched.
pub(crate) fn reseed_output_db_for_capture_change(
    parameters: &[GridParameter],
    captures: &[GridCapture],
    manifest_output_gain_db: Option<f32>,
    params: &ParameterSet,
    changed_path: &str,
) -> Option<f32> {
    if changed_path == "output_db" {
        return None;
    }
    let capture = plugin_loader::dispatch::resolve_capture(parameters, captures, params)?;
    capture.output_gain_db.or(manifest_output_gain_db)
}

/// Apply [`reseed_output_db_for_capture_change`] to a block in place. No-op
/// for non-IR blocks or when the pure resolver declines. The capture is
/// resolved against the block's *current* params (the axis write already
/// happened), so the new selection is reflected.
pub(crate) fn reseed_ir_output_db(block: &mut AudioBlock, changed_path: &str) {
    let new_output_db = {
        let Some(model) = block.model_ref() else {
            return;
        };
        if model.effect_type != block_core::EFFECT_TYPE_IR {
            return;
        }
        let Some(pkg) = plugin_loader::registry::find(&model.model) else {
            return;
        };
        let Backend::Ir {
            parameters,
            captures,
        } = &pkg.manifest.backend
        else {
            return;
        };
        reseed_output_db_for_capture_change(
            parameters,
            captures,
            pkg.manifest.output_gain_db,
            &model.params,
            changed_path,
        )
    };
    if let Some(db) = new_output_db {
        let _ = project::block::param_writer::set_parameter_number(block, "output_db", db as f64);
    }
}

#[cfg(test)]
#[path = "local_dispatcher_ir_reseed_tests.rs"]
mod tests;
