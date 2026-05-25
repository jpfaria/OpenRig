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
/// stored in `rig.chain_order` so a reorder via `Command::MoveChainUp` /
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
mod tests {
    //! Issue #502: prove `sync_synthetic_into_rig` captures the
    //! user-defined chain order so save+reload preserves a reorder.
    use super::*;
    use crate::chain::Chain;
    use crate::rig::{RigInput, RigPreset};
    use domain::ids::ChainId;
    use std::collections::BTreeMap;

    fn rig_with_inputs(names: &[&str]) -> RigProject {
        let mut inputs = BTreeMap::new();
        let mut presets = BTreeMap::new();
        for name in names {
            inputs.insert(
                (*name).to_string(),
                RigInput {
                    label: None,
                    sources: Vec::new(),
                    bank: BTreeMap::from([(1, format!("{name}_preset"))]),
                    active_preset: 1,
                    active_scene: 1,
                    routing: Vec::new(),
                },
            );
            presets.insert(
                format!("{name}_preset"),
                RigPreset {
                    id: String::new(),
                    name: None,
                    volume: 100.0,
                    blocks: Vec::new(),
                    scenes: BTreeMap::new(),
                    scene_params: Vec::new(),
                },
            );
        }
        RigProject {
            name: None,
            inputs,
            outputs: BTreeMap::new(),
            presets,
            midi: None,
            chain_order: Vec::new(),
        }
    }

    fn project_with_chain_ids(ids: &[&str]) -> Project {
        Project {
            name: None,
            device_settings: Vec::new(),
            chains: ids
                .iter()
                .map(|id| Chain {
                    id: ChainId((*id).into()),
                    description: None,
                    instrument: "electric_guitar".into(),
                    enabled: false,
                    volume: 100.0,
                    blocks: Vec::new(),
                })
                .collect(),
            midi: None,
        }
    }

    #[test]
    fn sync_captures_reordered_chains_in_chain_order() {
        // Inputs "a" and "b" — alphabetical order is ["a", "b"]. After
        // MoveChainUp on "b", project.chains = ["rig:b", "rig:a"].
        let mut rig = rig_with_inputs(&["a", "b"]);
        let proj = project_with_chain_ids(&["rig:b", "rig:a"]);

        sync_synthetic_into_rig(&mut rig, &proj);

        assert_eq!(rig.chain_order, vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn sync_leaves_chain_order_empty_when_alphabetical() {
        // Order matches the BTreeMap iteration — no need to write
        // chain_order. Keeps legacy `.openrig` files lean.
        let mut rig = rig_with_inputs(&["a", "b", "c"]);
        let proj = project_with_chain_ids(&["rig:a", "rig:b", "rig:c"]);

        sync_synthetic_into_rig(&mut rig, &proj);

        assert!(rig.chain_order.is_empty());
    }

    #[test]
    fn sync_drops_stale_chain_order_entries_not_in_inputs() {
        let mut rig = rig_with_inputs(&["a", "b"]);
        // Project somehow still references "c" — that input was already
        // removed from rig.inputs.
        let proj = project_with_chain_ids(&["rig:c", "rig:b", "rig:a"]);

        sync_synthetic_into_rig(&mut rig, &proj);

        assert_eq!(
            rig.chain_order,
            vec!["b".to_string(), "a".to_string()],
            "stale name must not leak into chain_order"
        );
    }

    #[test]
    fn sync_clears_chain_order_when_back_to_alphabetical() {
        // The rig was previously reordered to ["b", "a"]. The user
        // reorders it back to alphabetical → chain_order must reset to
        // empty so the YAML stays clean.
        let mut rig = rig_with_inputs(&["a", "b"]);
        rig.chain_order = vec!["b".to_string(), "a".to_string()];
        let proj = project_with_chain_ids(&["rig:a", "rig:b"]);

        sync_synthetic_into_rig(&mut rig, &proj);

        assert!(rig.chain_order.is_empty());
    }

    #[test]
    fn sync_ignores_non_rig_chain_ids() {
        // A legacy `Project` may still hold non-prefixed chain ids.
        let mut rig = rig_with_inputs(&["a"]);
        let proj = project_with_chain_ids(&["legacy_chain", "rig:a"]);

        sync_synthetic_into_rig(&mut rig, &proj);

        // Only "a" was projected from the rig; the projected list of
        // rig-prefixed chains equals the alphabetical order ⇒ empty.
        assert!(rig.chain_order.is_empty());
    }
}
