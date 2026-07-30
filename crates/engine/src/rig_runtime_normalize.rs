//! Which I/O blocks survive when a rig is projected into chains (#716 + #85).
//!
//! Split out of `rig_runtime.rs` (file cap). Two callers share this rule: the
//! chain build (`rig_to_chains`) and the looper's isolated playback chain.

use project::block::{AudioBlock, AudioBlockKind};
use project::rig::RigProject;

/// Whether `block` is a legacy leftover that duplicates the chain's OWN I/O.
///
/// #716: a binding-bound chain takes its head input and tail output from
/// `io_binding_ids`, so an `Input`/`Output` block pointing at one of those same
/// bindings is a duplicate — it opens a second stream on the same device
/// (absurd latency + underruns) and must be dropped on load.
///
/// #85: a port pointing at ANY OTHER binding is a mid port the user placed on
/// purpose (e.g. an aux send to a second E/S). It is not a duplicate and must
/// survive. An unbound port (`io` empty) is one the user just added and has not
/// pointed anywhere yet — it survives too, or it would vanish before it could
/// ever be configured.
pub(crate) fn duplicates_chain_binding(block: &AudioBlock, io_binding_ids: &[String]) -> bool {
    let io = match &block.kind {
        AudioBlockKind::Input(b) => &b.io,
        AudioBlockKind::Output(b) => &b.io,
        _ => return false,
    };
    !io.is_empty() && io_binding_ids.iter().any(|id| id == io)
}

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
