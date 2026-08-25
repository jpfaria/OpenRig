//! Responsibility: decides which I O blocks survive when a rig is projected.
//! Which I/O blocks survive when a rig is projected into chains (#716 + #85).
//!
//! Split out of `rig_runtime.rs` (file cap). Two callers share this rule: the
//! chain build (`rig_to_chains`) and the looper's isolated playback chain.

use project::block::AudioBlock;
use project::rig::RigProject;

// The rule itself lives in `project::block` — save (`rig_sync`) and load share
// it, or a mid port that survives one side gets dropped by the other (#85).
pub(crate) use project::block::duplicates_chain_binding;

/// #323 phase 2: the processing blocks a loop LINKED to `preset_id` plays
/// through, resolved against `rig`. Mirrors the chain build for one input: the
/// linked preset's blocks with the input's active scene applied, and — for a
/// binding-bound input (#716) — the chain's own I/O stripped, since the isolated
/// playback stream resolves that I/O from the bindings. Mid ports (#85) stay,
/// exactly as they do in the chain. `None` when the input or the linked preset
/// no longer exists (a deleted preset ⇒ the caller falls back to the chain's
/// current blocks).
pub fn looper_playback_blocks(
    rig: &RigProject,
    input_name: &str,
    preset_id: &str,
) -> Option<Vec<AudioBlock>> {
    let input = rig.inputs.get(input_name)?;
    let preset = rig.presets.get(preset_id)?;
    let mut blocks = preset.apply_scene(input.active_scene);
    if !input.io_binding_ids.is_empty() {
        blocks.retain(|b| !duplicates_chain_binding(b, &input.io_binding_ids));
    }
    Some(blocks)
}
