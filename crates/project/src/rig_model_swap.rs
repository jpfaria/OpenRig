//! Responsibility: writes a same-slot block model swap into the rig preset it came from.
//!
//! #986 — a model swap keeps the block id and position, so it is not a
//! structural edit. It used to be classified as one (#627), and the structural
//! write-back replaced the whole preset with the live chain: every scene and
//! `scene-params` entry was cleared and the live (scene-applied) values were
//! baked into the base. Here only the swapped block's base changes, and only
//! the scene overrides the new model does not have are dropped.

use std::collections::BTreeSet;

use crate::block::{AudioBlock, AudioBlockKind};
use crate::param::ParameterSet;
use crate::rig::RigProject;

fn params(kind: &AudioBlockKind) -> Option<&ParameterSet> {
    match kind {
        AudioBlockKind::Core(c) => Some(&c.params),
        AudioBlockKind::Nam(n) => Some(&n.params),
        _ => None,
    }
}

impl RigProject {
    /// Write every same-id, same-position model swap in `blocks` (the chain's
    /// processing blocks) into the input's active preset: the base block takes
    /// the new kind and keeps its own `enabled`, and the scene overrides and
    /// `scene-params` entries of that block whose parameter the new model lacks
    /// are removed. Everything else in the preset is left alone.
    ///
    /// Returns `blocks` ready for the per-scene diff: a swapped block is
    /// replaced by what the active scene resolves it to (keeping the live
    /// `enabled`), so the diff neither clears the active scene's surviving
    /// overrides nor bakes them into the base. When the block list differs
    /// by id or order (a structural edit) nothing is written and `blocks`
    /// comes back unchanged.
    pub fn write_back_model_swaps(
        &mut self,
        input: &str,
        blocks: Vec<AudioBlock>,
    ) -> Vec<AudioBlock> {
        let Some((preset_name, scene_idx)) = self.inputs.get(input).and_then(|ri| {
            ri.bank
                .get(&ri.active_preset)
                .cloned()
                .map(|n| (n, ri.active_scene))
        }) else {
            return blocks;
        };
        let Some(preset) = self.presets.get_mut(&preset_name) else {
            return blocks;
        };
        let same_slots = preset.blocks.len() == blocks.len()
            && preset.blocks.iter().zip(&blocks).all(|(a, b)| a.id == b.id);
        if !same_slots {
            return blocks;
        }

        let mut swapped = BTreeSet::new();
        for (base, edited) in preset.blocks.iter_mut().zip(&blocks) {
            if base.kind.model_identity() == edited.kind.model_identity() {
                continue;
            }
            base.kind = edited.kind.clone();
            swapped.insert(base.id.0.clone());

            let bid = &base.id.0;
            let prefix = format!("{bid}.");
            let kept = |key: &str| match key.strip_prefix(&prefix) {
                Some(param) => params(&edited.kind).is_some_and(|p| p.get(param).is_some()),
                None => true,
            };
            for scene in preset.scenes.values_mut() {
                scene.params.retain(|key, _| kept(key));
            }
            preset.scene_params.retain(|key| kept(key));
        }
        if swapped.is_empty() {
            return blocks;
        }

        let resolved = preset.apply_scene(scene_idx);
        blocks
            .into_iter()
            .zip(resolved)
            .map(|(live, scene_view)| {
                if swapped.contains(&live.id.0) {
                    AudioBlock {
                        enabled: live.enabled,
                        ..scene_view
                    }
                } else {
                    live
                }
            })
            .collect()
    }
}
