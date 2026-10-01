//! Responsibility: changes how many paths a split has.
//!
//! #328 (spec §11.1): `AddSplitPath` appends an empty path with its default
//! knobs; `RemoveSplitPath` drops one path with its blocks, then renumbers
//! everything that addressed the paths above it — the split's knobs, the
//! leaf output checklists, the MIDI bindings and the active preset's scenes.

use anyhow::{anyhow, Result};

use domain::ids::{BlockId, ChainId};
use project::block::split_param_renumber::drop_path_keys;
use project::block::split_params::normalize_split_params;
use project::block::{walk_blocks, y_leaves, SplitBlock, MIN_SPLIT_PATHS};

use crate::block_path::split_mut;
use crate::local_dispatcher::LocalDispatcher;
use crate::split_path_references::{rewrite_midi_bindings, rewrite_preset_scenes, RemovedPath};

/// Append an empty path to `split`; the knobs of the new path take their
/// defaults and the others keep their values.
pub(crate) fn add_split_path(split: &mut SplitBlock) {
    split.paths.push(Vec::new());
    if let Ok(params) = normalize_split_params(split.params.clone(), split.paths.len()) {
        split.params = params;
    }
}

impl LocalDispatcher {
    pub(crate) fn remove_split_path(
        &self,
        chain: &ChainId,
        split_id: &BlockId,
        path: usize,
    ) -> Result<()> {
        let gone = self.edit_chain_blocks(chain, |blocks| {
            let split = split_mut(blocks, split_id)?;
            if path >= split.paths.len() {
                return Err(anyhow!(
                    "split {:?} has {} paths; path {path} does not exist",
                    split_id,
                    split.paths.len()
                ));
            }
            if split.paths.len() <= MIN_SPLIT_PATHS {
                return Err(anyhow!("a split keeps at least {MIN_SPLIT_PATHS} paths"));
            }
            let removed = split.paths.remove(path);
            split.params = drop_path_keys(&split.params, path);
            Ok(walk_blocks(&removed)
                .into_iter()
                .map(|block| block.id.clone())
                .collect::<Vec<_>>())
        })?;
        let removed = RemovedPath {
            split: split_id,
            path,
            gone: &gone,
        };
        self.with_chain(chain, |c| {
            c.disabled_endpoints
                .shift_after_path_removed(split_id, path);
            c.disabled_endpoints.retain_leaves(&y_leaves(&c.blocks));
            Ok(())
        })?;
        if let Some(midi) = self.project.borrow_mut().midi.as_mut() {
            rewrite_midi_bindings(&mut midi.bindings, &removed);
        }
        self.rewrite_active_preset_scenes(chain, &removed);
        Ok(())
    }

    /// The scenes of the preset the chain is playing address its blocks by id.
    fn rewrite_active_preset_scenes(&self, chain: &ChainId, removed: &RemovedPath<'_>) {
        let Some(input) = chain.0.strip_prefix("rig:") else {
            return;
        };
        let Some(rig) = self.rig.borrow().clone() else {
            return;
        };
        let mut rig = rig.borrow_mut();
        let Some(key) = rig
            .inputs
            .get(input)
            .and_then(|ri| ri.bank.get(&ri.active_preset).cloned())
        else {
            return;
        };
        if let Some(preset) = rig.presets.get_mut(&key) {
            rewrite_preset_scenes(preset, removed);
        }
    }
}
