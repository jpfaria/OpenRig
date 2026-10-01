//! Responsibility: rewrites what points at a split's paths once one of them is removed.
//!
//! #328 (spec §11.1): MIDI bindings and scene values address a split knob as
//! `<split-id>` + knob key, and a path block by its id. Removing path `i`
//! drops what addressed that path or its blocks, and renames the knobs of
//! the paths above it one index down, like the split's own params.

use domain::ids::BlockId;
use project::block::split_param_keys::renumbered_key;
use project::midi::Binding;
use project::rig::RigPreset;

/// What one removed path leaves behind: the split it belonged to, its index
/// and the ids of every block it held, nested ones included.
pub(crate) struct RemovedPath<'a> {
    pub split: &'a BlockId,
    pub path: usize,
    pub gone: &'a [BlockId],
}

impl RemovedPath<'_> {
    /// The knob `key` of `block` after the removal: `None` when it is gone.
    fn knob(&self, block: &str, key: &str) -> Option<String> {
        if self.gone.iter().any(|id| id.0 == block) {
            return None;
        }
        if block == self.split.0 {
            return renumbered_key(key, self.path);
        }
        Some(key.to_string())
    }

    /// A `<block-id>.<knob>` scene key after the removal.
    fn scene_key(&self, key: &str) -> Option<String> {
        if let Some(knob) = key.strip_prefix(&format!("{}.", self.split.0)) {
            return renumbered_key(knob, self.path).map(|k| format!("{}.{k}", self.split.0));
        }
        let gone = self.gone.iter().any(|id| {
            key.strip_prefix(&id.0)
                .is_some_and(|rest| rest.starts_with('.'))
        });
        (!gone).then(|| key.to_string())
    }
}

/// Drop the bindings that drive the removed path, rename the ones that
/// drive a knob of a path above it.
pub(crate) fn rewrite_midi_bindings(bindings: &mut Vec<Binding>, removed: &RemovedPath<'_>) {
    bindings.retain_mut(|binding| {
        let Some(block) = binding.args.get("block").and_then(|v| v.as_str()) else {
            return true;
        };
        let block = block.to_string();
        let Some(key) = binding.args.get("path").and_then(|v| v.as_str()) else {
            return !removed.gone.iter().any(|id| id.0 == block);
        };
        match removed.knob(&block, key) {
            Some(new_key) => {
                binding.args["path"] = serde_json::Value::String(new_key);
                true
            }
            None => false,
        }
    });
}

/// Drop the scene entries of the removed path, rename the knobs above it.
pub(crate) fn rewrite_preset_scenes(preset: &mut RigPreset, removed: &RemovedPath<'_>) {
    preset.scene_params = preset
        .scene_params
        .iter()
        .filter_map(|key| removed.scene_key(key))
        .collect();
    for scene in preset.scenes.values_mut() {
        scene.params = std::mem::take(&mut scene.params)
            .into_iter()
            .filter_map(|(key, value)| removed.scene_key(&key).map(|k| (k, value)))
            .collect();
        scene
            .bypass
            .retain(|block, _| !removed.gone.iter().any(|id| id.0 == *block));
    }
}
