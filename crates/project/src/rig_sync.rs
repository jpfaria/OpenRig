//! #436 architectural fix: capturing the projected synthetic chains
//! back into the `RigProject` is pure model logic — it must live in the
//! `project` crate so the dispatcher (and any non-GUI adapter) can run
//! it, instead of being buried in `adapter-gui`. `adapter-gui` re-exports
//! it at the old path so existing tests/callers don't move.

use crate::block::AudioBlockKind;
use crate::project::Project;
use crate::rig::RigProject;

/// Write every rig chain's edited processing blocks **and chain volume**
/// back into the rig's active preset, per active scene, so edits made on
/// the projected synthetic chains survive re-projection and are saved to
/// `project.openrig`. Non-rig chains are ignored. Pure; mirrors
/// `rig_to_chains` in reverse.
pub fn sync_synthetic_into_rig(rig: &mut RigProject, project: &Project) {
    for chain in &project.chains {
        let Some(input) = chain.id.0.strip_prefix("rig:") else {
            continue;
        };
        let processing: Vec<_> = chain
            .blocks
            .iter()
            .filter(|b| !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
            .cloned()
            .collect();
        // Structural change (preset loaded over the slot / blocks
        // added-removed-reordered) replaces the preset base; otherwise
        // it's a per-scene param/bypass diff. Without the structural
        // branch a loaded preset never persisted — its new block ids
        // matched nothing in the diff base.
        if !rig.replace_preset_blocks_if_structural(input, &processing) {
            rig.write_back_processing_blocks(input, processing);
        }
        rig.write_back_chain_volume(input, chain.volume);
        // The synthetic Input block carries `RigInput.sources`; an edit
        // there (added device/channel) was being dropped because the
        // loop only wrote processing blocks back. Persist it too.
        if let Some(entries) = chain.blocks.iter().find_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) if !ib.entries.is_empty() => Some(ib.entries.clone()),
            _ => None,
        }) {
            rig.set_input_sources(input, entries);
        }
    }
}
