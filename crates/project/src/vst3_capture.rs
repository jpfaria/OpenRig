//! #780: capture live VST3 controller values into each block's `ParameterSet`
//! so parameters changed in a plugin's native editor persist to the project.
//!
//! Runs on the save path (`Command::CaptureRigEdits`), never on the audio
//! thread. `capture_live_vst3_params_with` is pure and takes the live-value
//! reader as a seam, so the fold is unit-testable without a live plugin; the
//! production wrapper injects `vst3_host::capture_vst3_params`.

use crate::block::AudioBlockKind;
use crate::project::Project;
use domain::value_objects::ParameterValue;

/// Is `path` a VST3 stored-param key (`p{digits}`)?
fn is_vst3_param_path(path: &str) -> bool {
    path.strip_prefix('p')
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

/// Fold live VST3 controller values into each VST3 block's params. For every
/// core block whose `effect_type` is VST3, `reader(&block.id.0)` returns the
/// current `(param_id, normalized)` pairs (non-default) for that block's live
/// instance; each is written as `p{id}` = percent. The block's existing
/// `p{id}` entries are replaced wholesale so a param returned to default drops
/// out. Blocks with no live context (`reader` returns `None`) are left as-is.
pub fn capture_live_vst3_params_with(
    project: &mut Project,
    reader: impl Fn(&str) -> Option<Vec<(u32, f64)>>,
) {
    for chain in &mut project.chains {
        for block in &mut chain.blocks {
            let AudioBlockKind::Core(core) = &mut block.kind else {
                continue;
            };
            if core.effect_type != block_core::EFFECT_TYPE_VST3 {
                continue;
            }
            let Some(values) = reader(&block.id.0) else {
                continue;
            };
            core.params
                .values
                .retain(|path, _| !is_vst3_param_path(path));
            for (id, normalized) in values {
                core.params.insert(
                    format!("p{id}"),
                    ParameterValue::Float((normalized * 100.0) as f32),
                );
            }
        }
    }
}

/// Production entry point: capture using the global VST3 registry. Called from
/// the dispatcher's `CaptureRigEdits` handler before serializing.
pub fn capture_live_vst3_params(project: &mut Project) {
    capture_live_vst3_params_with(project, |key| vst3_host::capture_vst3_params(key));
}
