//! Responsibility: moves an input between its presets.
//!
//! Split out of `rig_methods.rs` (#873).

use crate::rig::{RigPreset, RigProject, RigScene};

impl RigProject {
    /// New preset position (0-based ordinal into the input's ascending
    /// bank) after stepping the active preset by `delta`, wrapping.
    /// `None` if the input is unknown or its bank is empty. The single
    /// source of the footswitch next/previous wrap math.
    pub fn step_preset(&self, input: &str, delta: i32) -> Option<usize> {
        let ri = self.inputs.get(input)?;
        let len = ri.bank.len();
        if len == 0 {
            return None;
        }
        let cur = ri.bank.keys().position(|k| *k == ri.active_preset)?;
        Some((cur as i32 + delta).rem_euclid(len as i32) as usize)
    }

    /// New scene number (`1..=scene_count` of the active preset) after
    /// stepping the active scene by `delta`, wrapping. `None` if the
    /// input or its active preset is unknown.
    pub fn step_scene(&self, input: &str, delta: i32) -> Option<usize> {
        let ri = self.inputs.get(input)?;
        let name = ri.bank.get(&ri.active_preset)?;
        let count = self.presets.get(name)?.scene_count() as i32;
        let cur = ri.active_scene as i32 - 1;
        Some((cur + delta).rem_euclid(count) as usize + 1)
    }

    /// Add a new preset to `input`'s bank: takes the next free slot
    /// (max key + 1, or 1 for an empty bank), gets a unique name, and
    /// makes the new slot active. The new preset starts **fresh** —
    /// no blocks, default volume, single Default scene. Cloning the
    /// active preset was confusing: switching to the new slot looked
    /// identical to the previous one, so the "+" button felt broken.
    /// Returns the new slot, or `None` if the input is unknown.
    pub fn add_preset_to_input(&mut self, input: &str) -> Option<usize> {
        let ri = self.inputs.get(input)?;
        let slot = ri.bank.keys().max().map(|m| m + 1).unwrap_or(1);
        let template = RigPreset::from_legacy_blocks(Vec::new(), 100.0);
        let name = self.unique_preset_name("New Preset");
        self.presets.insert(name.clone(), template);
        let ri = self.inputs.get_mut(input)?;
        ri.bank.insert(slot, name);
        ri.active_preset = slot;
        ri.active_scene = 1;
        Some(slot)
    }

    /// Add the next scene to `input`'s active preset. Scenes grow on
    /// demand (a preset starts with just scene 1); the new scene is an
    /// **independent snapshot** of the currently active scene (same
    /// bypass/params, and its volume frozen to the active scene's
    /// effective volume) so editing it never bleeds back. Becomes the
    /// active scene. `None` if the input/preset is unknown or already
    /// at the 8-scene maximum.
    pub fn add_scene_to_input(&mut self, input: &str) -> Option<usize> {
        let (preset_name, active_scene) = self.inputs.get(input).and_then(|ri| {
            ri.bank
                .get(&ri.active_preset)
                .map(|n| (n.clone(), ri.active_scene))
        })?;
        let preset = self.presets.get_mut(&preset_name)?;
        let next = preset.scene_count() + 1;
        if next > 8 {
            return None;
        }
        let snapshot = RigScene {
            volume: Some(preset.scene_volume(active_scene)),
            ..preset.scene_or_default(active_scene)
        };
        preset.scenes.insert(next, snapshot);
        self.inputs.get_mut(input)?.active_scene = next;
        Some(next)
    }

    /// Remove an entire input (a "chain" on the legacy screen). Presets
    /// it banked are dropped from the shared pool unless another input
    /// still references them. Returns `true` if the input existed —
    /// `false` is a no-op (so the GUI can ignore a stale delete).
    pub fn remove_input(&mut self, input: &str) -> bool {
        if self.inputs.remove(input).is_none() {
            return false;
        }
        let inputs = &self.inputs;
        self.presets
            .retain(|name, _| inputs.values().any(|i| i.bank.values().any(|n| n == name)));
        true
    }

    /// Remove the **active** preset from `input`'s bank. The last
    /// remaining preset can't be removed (a bank must keep ≥ 1). The
    /// largest remaining slot becomes active. If the removed preset name
    /// is no longer referenced by ANY input bank, it's dropped from the
    /// shared pool (no orphan). Returns the new active slot, or `None`
    /// if the input is unknown or only one preset remains.
    pub fn remove_preset_from_input(&mut self, input: &str) -> Option<usize> {
        let ri = self.inputs.get(input)?;
        if ri.bank.len() <= 1 {
            return None;
        }
        let active = ri.active_preset;
        let removed_name = ri.bank.get(&active)?.clone();
        let ri = self.inputs.get_mut(input)?;
        ri.bank.remove(&active);
        let new_active = *ri.bank.keys().max()?;
        ri.active_preset = new_active;
        ri.active_scene = 1;
        // Drop the pool entry only if nothing references it anymore.
        let still_used = self
            .inputs
            .values()
            .any(|i| i.bank.values().any(|n| *n == removed_name));
        if !still_used {
            self.presets.remove(&removed_name);
        }
        Some(new_active)
    }

    /// Remove the **last** scene of `input`'s active preset (stack pop,
    /// mirrors [`Self::add_scene_to_input`]). Keeps scene indices a
    /// dense `1..=scene_count` range. The single remaining scene can't
    /// be removed. Returns the (possibly clamped) active scene, or
    /// `None` if the input/preset is unknown or only one scene exists.
    pub fn remove_last_scene_from_input(&mut self, input: &str) -> Option<usize> {
        let preset_name = self
            .inputs
            .get(input)
            .and_then(|ri| ri.bank.get(&ri.active_preset).cloned())?;
        let preset = self.presets.get_mut(&preset_name)?;
        let last = preset.scene_count();
        if last <= 1 {
            return None;
        }
        preset.scenes.remove(&last);
        let ri = self.inputs.get_mut(input)?;
        if ri.active_scene >= last {
            ri.active_scene = last - 1;
        }
        Some(ri.active_scene)
    }

    /// A preset-pool name not yet in use: `base`, else `base 2`, `base 3`…
    fn unique_preset_name(&self, base: &str) -> String {
        if !self.presets.contains_key(base) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|c| !self.presets.contains_key(c))
            .expect("infinite range always yields a free name")
    }
}
