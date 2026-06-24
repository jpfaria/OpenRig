//! Methods on [`crate::rig::RigProject`] — extracted from `rig.rs` so the
//! type-definitions file stays under the 600-line cap (validate.sh).
//!
//! Pure refactor: no behavior change. Tests live in `rig_tests.rs` and
//! `rig_scene_tests.rs`.

use crate::block::{AudioBlock, AudioBlockKind};
use crate::rig::{RigPreset, RigProject, RigScene};
use domain::value_objects::ParameterValue;
use std::collections::BTreeMap;

impl RigProject {
    /// Persist a block/param edit made on the projected synthetic chain
    /// back into the active preset, **per scene (snapshot semantics)**:
    /// the edit is captured into the input's *active scene* only, so each
    /// scene keeps its own values. `preset.blocks` stays the factory
    /// template; a float param / bypass that differs from the template is
    /// stored as that scene's override (and the key auto-marked as a
    /// scene-param so `apply_scene` applies it). A value back at the
    /// template clears the override. Non-float params (Bool/Int/String)
    /// cannot live in the f32 scene diff — they are written into the
    /// preset base itself, shared by every scene (issue #690). No-op if
    /// input/preset is unknown.
    pub fn write_back_processing_blocks(
        &mut self,
        input: &str,
        blocks: Vec<crate::block::AudioBlock>,
    ) {
        let Some((preset_name, scene_idx)) = self.inputs.get(input).and_then(|ri| {
            ri.bank
                .get(&ri.active_preset)
                .cloned()
                .map(|n| (n, ri.active_scene))
        }) else {
            return;
        };
        let Some(preset) = self.presets.get_mut(&preset_name) else {
            return;
        };

        // Factory template, indexed by block id (immutable diff base).
        let base: BTreeMap<String, AudioBlock> = preset
            .blocks
            .iter()
            .map(|b| (b.id.0.clone(), b.clone()))
            .collect();

        let mut set_param: Vec<(String, f32)> = Vec::new();
        let mut clear_param: Vec<String> = Vec::new();
        let mut set_base_param: Vec<(String, String, ParameterValue)> = Vec::new();
        let mut set_bypass: Vec<(String, bool)> = Vec::new();
        let mut clear_bypass: Vec<String> = Vec::new();

        for edited in &blocks {
            let bid = edited.id.0.clone();
            let Some(base_blk) = base.get(&bid) else {
                continue;
            };
            if edited.enabled != base_blk.enabled {
                set_bypass.push((bid.clone(), !edited.enabled));
            } else {
                clear_bypass.push(bid.clone());
            }
            let pair = match (&edited.kind, &base_blk.kind) {
                (AudioBlockKind::Core(e), AudioBlockKind::Core(b)) => Some((&e.params, &b.params)),
                (AudioBlockKind::Nam(e), AudioBlockKind::Nam(b)) => Some((&e.params, &b.params)),
                _ => None,
            };
            if let Some((ep, bp)) = pair {
                for (pid, val) in &ep.values {
                    match val {
                        ParameterValue::Float(v) => {
                            let key = format!("{bid}.{pid}");
                            if bp.get_f32(pid) != Some(*v) {
                                set_param.push((key, *v));
                            } else {
                                clear_param.push(key);
                            }
                        }
                        // Scenes can only carry f32 overrides (Helix
                        // snapshot rule), so a Bool/Int/String/enum edit
                        // is preset-level: write it into the base
                        // template, shared by every scene. Issue #690 —
                        // the NAM noise-gate toggle was silently dropped
                        // here and reverted on save+reload.
                        other => {
                            if bp.get(pid) != Some(other) {
                                set_base_param.push((bid.clone(), pid.clone(), other.clone()));
                            }
                        }
                    }
                }
            }
        }

        for (bid, pid, val) in set_base_param {
            let params =
                preset
                    .blocks
                    .iter_mut()
                    .find(|b| b.id.0 == bid)
                    .and_then(|b| match &mut b.kind {
                        AudioBlockKind::Core(c) => Some(&mut c.params),
                        AudioBlockKind::Nam(n) => Some(&mut n.params),
                        _ => None,
                    });
            if let Some(params) = params {
                params.insert(pid, val);
            }
        }

        let scene = preset.scenes.entry(scene_idx).or_default();
        for (b, v) in &set_bypass {
            scene.bypass.insert(b.clone(), *v);
        }
        for b in &clear_bypass {
            scene.bypass.remove(b);
        }
        for (k, v) in &set_param {
            scene.params.insert(k.clone(), *v);
        }
        for k in &clear_param {
            scene.params.remove(k);
        }
        for (k, _) in &set_param {
            if !preset.scene_params.contains(k) {
                preset.scene_params.push(k.clone());
            }
        }
    }

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

    /// Replace the active preset's base blocks when `blocks` is a
    /// **structural** change (different block ids/order/count vs the
    /// preset's base) — e.g. a preset was loaded over the slot, or
    /// blocks were added/removed/reordered. `write_back_processing_blocks`
    /// is diff-only (param/bypass keyed by block id) and silently drops
    /// such edits, so they never persisted. Scenes/scene-params reference
    /// the OLD structure, so they are reset. Returns `true` when it
    /// replaced (the caller then skips the per-scene diff write-back for
    /// this input). No-op / `false` if the input/preset is unknown or
    /// the structure is identical (id-for-id) — that path stays diff-only.
    pub fn replace_preset_blocks_if_structural(
        &mut self,
        input: &str,
        blocks: &[AudioBlock],
    ) -> bool {
        let Some(preset_name) = self
            .inputs
            .get(input)
            .and_then(|ri| ri.bank.get(&ri.active_preset).cloned())
        else {
            return false;
        };
        let Some(preset) = self.presets.get_mut(&preset_name) else {
            return false;
        };
        // "Same structure" requires both the same id AND the same model
        // identity. A `ReplaceBlockModel` keeps the id but changes the model
        // (#627); comparing ids alone classified that as a non-structural
        // per-scene diff, so the swapped model was never written into the
        // preset base and reverted on reload. Model identity excludes params,
        // so genuine param/bypass edits still take the diff-only path below.
        let same_structure =
            preset.blocks.len() == blocks.len()
                && preset.blocks.iter().zip(blocks).all(|(a, b)| {
                    a.id == b.id && a.kind.model_identity() == b.kind.model_identity()
                });
        if same_structure {
            return false;
        }
        preset.blocks = blocks.to_vec();
        preset.scenes.clear();
        preset.scene_params.clear();
        true
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

    /// Persist the chain volume edited on the projected synthetic chain
    /// back into the active preset, **per active scene** (snapshot
    /// semantics — mirrors [`Self::write_back_processing_blocks`]). A
    /// value equal to the preset volume clears the per-scene override
    /// (no stale snapshot); anything else is stored for that scene only.
    /// No-op if the input/preset is unknown.
    pub fn write_back_chain_volume(&mut self, input: &str, volume: f32) {
        let Some((preset_name, scene_idx)) = self.inputs.get(input).and_then(|ri| {
            ri.bank
                .get(&ri.active_preset)
                .cloned()
                .map(|n| (n, ri.active_scene))
        }) else {
            return;
        };
        let Some(preset) = self.presets.get_mut(&preset_name) else {
            return;
        };
        let base = preset.volume;
        let scene = preset.scenes.entry(scene_idx).or_default();
        scene.volume = if (volume - base).abs() < f32::EPSILON {
            None
        } else {
            Some(volume)
        };
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

    /// Validate cross-references in the rig model.
    ///
    /// Rules (closed in #436 / scoped by #449; device-channel conflicts
    /// moved to runtime activation in #716):
    /// 1. every `bank` value must name a preset in `presets`;
    /// 2. each input's `active_preset` must be a key in its own `bank`;
    /// 3. each input's `active_scene` ∈ `1..=8`;
    /// 4. no preset may contain an `Input`/`Output` block;
    /// 5. every `routing` target must name an `outputs` entry.
    ///
    /// Device endpoints no longer live in the model (model A, #716), so any
    /// capture/output exclusivity is enforced by the engine at runtime
    /// against the per-machine binding registry, not by this static model.
    pub fn validate(&self) -> Result<(), String> {
        for (name, input) in &self.inputs {
            for (idx, preset_name) in &input.bank {
                if !self.presets.contains_key(preset_name) {
                    return Err(format!(
                        "input '{name}' bank slot {idx} references unknown preset '{preset_name}'"
                    ));
                }
            }
            if !input.bank.contains_key(&input.active_preset) {
                return Err(format!(
                    "input '{name}' active-preset {} is not a slot in its bank",
                    input.active_preset
                ));
            }
            if !(1..=8).contains(&input.active_scene) {
                return Err(format!(
                    "input '{name}' active-scene {} out of range 1..=8",
                    input.active_scene
                ));
            }
            for target in &input.routing {
                if !self.outputs.contains_key(target) {
                    return Err(format!(
                        "input '{name}' routes to unknown output '{target}'"
                    ));
                }
            }
        }
        for (name, preset) in &self.presets {
            for block in &preset.blocks {
                if matches!(
                    block.kind,
                    AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
                ) {
                    return Err(format!(
                        "preset '{name}' contains an I/O block ({}); presets are processing-only",
                        block.kind.label()
                    ));
                }
            }
            for (idx, scene) in &preset.scenes {
                if !(1..=8).contains(idx) {
                    return Err(format!("preset '{name}' scene {idx} out of range 1..=8"));
                }
                for key in scene.params.keys() {
                    if !preset.scene_params.contains(key) {
                        return Err(format!(
                            "preset '{name}' scene {idx} sets '{key}' which is not a marked scene-param"
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
