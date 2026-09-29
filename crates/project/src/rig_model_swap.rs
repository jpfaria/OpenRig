//! Responsibility: writes a same-slot block model swap into the rig preset it came from.
//!
//! #986 — a model swap keeps the block id and position, so it is not a
//! structural edit. It used to be classified as one (#627), and the structural
//! write-back replaced the whole preset with the live chain: every scene and
//! `scene-params` entry was cleared and the live (scene-applied) values were
//! baked into the base. Here only the swapped block's base changes, and only
//! the scene overrides the new model does not have are dropped.
//!
//! #328: the blocks inside a split's paths are slots too. A model swap on one
//! of them takes this path; a split whose paths changed shape (a block added,
//! removed or moved) is structural and is left to the structural write-back.

use crate::block::{block_params, for_each_block_mut, walk_blocks, AudioBlock, AudioBlockKind};
use crate::rig::RigProject;

/// The slot shape of a block list: every block id in walk order and, for a
/// split, its end and the length of each path — so moving a block from path A
/// to path B is a different shape even though the walk visits the same ids.
fn slot_layout(blocks: &[AudioBlock]) -> Vec<String> {
    walk_blocks(blocks)
        .into_iter()
        .map(|b| match &b.kind {
            AudioBlockKind::Split(split) => format!(
                "{}|{}|{}|{}",
                b.id.0,
                split.end.as_str(),
                split.a.len(),
                split.b.len()
            ),
            _ => b.id.0.clone(),
        })
        .collect()
}

impl RigProject {
    /// Write every same-id, same-position model swap in `blocks` (the chain's
    /// processing blocks, split paths included) into the input's active
    /// preset: the base block takes the new kind and keeps its own `enabled`,
    /// and the scene overrides and `scene-params` entries of that block whose
    /// parameter the new model lacks are removed. Everything else in the
    /// preset is left alone.
    ///
    /// Returns `blocks` ready for the per-scene diff: a swapped block is
    /// replaced by what the active scene resolves it to (keeping the live
    /// `enabled`), so the diff neither clears the active scene's surviving
    /// overrides nor bakes them into the base. When the slot layout differs
    /// (a structural edit) nothing is written and `blocks` comes back
    /// unchanged.
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
        if slot_layout(&preset.blocks) != slot_layout(&blocks) {
            return blocks;
        }

        // Every slot whose model changed, with the kind it takes. A split's
        // identity carries its paths, so it differs whenever a path block was
        // swapped — the swap is written on that path block instead.
        let mut swapped: Vec<(String, AudioBlockKind)> = Vec::new();
        {
            let live = walk_blocks(&blocks);
            let mut slot = 0;
            for_each_block_mut(&mut preset.blocks, &mut |base| {
                let edited = live[slot];
                slot += 1;
                if matches!(base.kind, AudioBlockKind::Split(_))
                    || base.kind.model_identity() == edited.kind.model_identity()
                {
                    return;
                }
                base.kind = edited.kind.clone();
                swapped.push((base.id.0.clone(), edited.kind.clone()));
            });
        }
        if swapped.is_empty() {
            return blocks;
        }

        for (bid, kind) in &swapped {
            let prefix = format!("{bid}.");
            let kept = |key: &str| match key.strip_prefix(&prefix) {
                Some(param) => block_params(kind).is_some_and(|p| p.get(param).is_some()),
                None => true,
            };
            for scene in preset.scenes.values_mut() {
                scene.params.retain(|key, _| kept(key));
            }
            preset.scene_params.retain(|key| kept(key));
        }

        let resolved_blocks = preset.apply_scene(scene_idx);
        let resolved = walk_blocks(&resolved_blocks);
        let mut live = blocks;
        for_each_block_mut(&mut live, &mut |block| {
            if !swapped.iter().any(|(id, _)| *id == block.id.0) {
                return;
            }
            if let Some(scene_view) = resolved.iter().find(|b| b.id == block.id) {
                *block = AudioBlock {
                    enabled: block.enabled,
                    ..(*scene_view).clone()
                };
            }
        });
        live
    }
}
