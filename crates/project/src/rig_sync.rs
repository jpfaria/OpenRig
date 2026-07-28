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
///
/// Also captures the user-defined chain order (issue #502, regression of
/// #246). The `rig:` prefix is stripped from each chain id and the result
/// stored in `rig.chain_order` so a reorder via `ChainCommand::MoveChainUp` /
/// `MoveChainDown` survives save+reload. When the projected chain list
/// matches the alphabetical `inputs` order exactly, `chain_order` is
/// cleared so legacy `.openrig` files keep their lean shape.
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
        // Capture the instrument type so it survives save+reload (#627).
        // #716: capture the selected I/O bindings so the editor checklist
        // selection survives reopen (the rig is the persistence model).
        if let Some(rig_input) = rig.inputs.get_mut(input) {
            rig_input.instrument = chain.instrument.clone();
            rig_input.io_binding_ids = chain.io_binding_ids.clone();
        }
    }
    sync_chain_order(rig, project);
}

/// Capture the user-defined chain order into `rig.chain_order`. Only
/// the input names that actually exist in `rig.inputs` are kept (a stale
/// entry from a removed chain would otherwise leak into the YAML). When
/// the projected list matches the alphabetical `inputs` order, the field
/// is left empty so default `.openrig` files don't grow a redundant key.
fn sync_chain_order(rig: &mut RigProject, project: &Project) {
    let order: Vec<String> = project
        .chains
        .iter()
        .filter_map(|c| c.id.0.strip_prefix("rig:").map(String::from))
        .filter(|name| rig.inputs.contains_key(name))
        .collect();
    let alphabetical: Vec<&String> = rig.inputs.keys().collect();
    let matches_alphabetical = order.len() == alphabetical.len()
        && order.iter().zip(alphabetical.iter()).all(|(a, b)| a == *b);
    if matches_alphabetical {
        rig.chain_order.clear();
    } else {
        rig.chain_order = order;
    }
}

#[cfg(test)]
#[path = "rig_sync_tests.rs"]
mod tests;
