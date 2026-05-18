//! `project.openrig` — project-level I/O + per-input preset banks (#436).
//!
//! Wraps the existing `InputEntry`/`OutputEntry`/`AudioBlock` model 1:1 — single
//! source of truth, zero duplication. The legacy chain-based
//! [`crate::project::Project`] is untouched; migration is #450.
//!
//! Scope of #449: model + parser + validation only. No engine wiring, no
//! migration, no UI, no scenes (those are #450/#451/#452/#453/#454).

use crate::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputEntry};
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn default_scene() -> usize {
    1
}

/// Root of a `project.openrig` document (under the top-level `project:` key).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigProject {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Named inputs. `BTreeMap` ⇒ deterministic round-trip ordering.
    #[serde(default)]
    pub inputs: BTreeMap<String, RigInput>,
    /// Named physical outputs.
    #[serde(default)]
    pub outputs: BTreeMap<String, RigOutput>,
    /// Shared preset pool — processing only, no I/O.
    #[serde(default)]
    pub presets: BTreeMap<String, RigPreset>,
}

/// One project input: a list of capture sources + a numbered preset bank.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// `= Vec<InputEntry>` de hoje. NÃO achatar pra device/channel único —
    /// `mode` é por fonte (invariante #4/multi-source de #436).
    pub sources: Vec<InputEntry>,
    /// Banco numerado: índice → nome do preset. Gaps permitidos.
    #[serde(default)]
    pub bank: BTreeMap<usize, String>,
    /// Índice ativo no `bank` (não o nome — o mesmo preset reusa em vários inputs).
    #[serde(rename = "active-preset")]
    pub active_preset: usize,
    /// Cena ativa, `1..=8`. Estrutura de cenas em si é #454.
    #[serde(rename = "active-scene", default = "default_scene")]
    pub active_scene: usize,
    /// Nomes de `outputs` para onde este input roteia.
    #[serde(default)]
    pub routing: Vec<String>,
}

/// One project output. Maps 1:1 onto the existing [`OutputEntry`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(flatten)]
    pub entry: OutputEntry,
}

/// One scene = only the *diff* over the base preset (Helix Snapshot style).
/// Anything not listed here is fixed by the preset and identical across scenes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RigScene {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// block-id → bypassed in this scene.
    #[serde(default)]
    pub bypass: BTreeMap<String, bool>,
    /// `"<block-id>.<param-id>"` → value. Must be a subset of `scene_params`.
    #[serde(default)]
    pub params: BTreeMap<String, f32>,
    /// Per-scene chain volume %. `None` ⇒ inherit the preset volume
    /// (back-compat: pre-#436 docs have no field → unchanged audio).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
}

fn default_preset_volume() -> f32 {
    100.0
}

/// Single source of truth for the on-disk format versions. Bumped only
/// when the YAML schema changes in a way that needs a staged upgrade;
/// the loader uses these to migrate older docs and to refuse newer ones.
pub const PROJECT_FORMAT_VERSION: u32 = 1;
/// See [`PROJECT_FORMAT_VERSION`]; the standalone preset file schema.
pub const PRESET_FORMAT_VERSION: u32 = 1;

/// A preset in the shared pool: processing chain only, no I/O.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigPreset {
    #[serde(default)]
    pub blocks: Vec<crate::block::AudioBlock>,
    /// Params the scenes are allowed to control (`<block-id>.<param-id>`).
    /// Everything else is fixed by the preset (Helix Snapshot rule).
    #[serde(rename = "scene-params", default)]
    pub scene_params: Vec<String>,
    /// `1..=8`. Empty ⇒ a single implicit "Default" scene (index 1).
    #[serde(default)]
    pub scenes: BTreeMap<usize, RigScene>,
    /// Output volume %, 100 = unity. Carried from `Chain.volume` on migration
    /// so master gain is unchanged (CLAUDE.md invariant). Default 100.0.
    #[serde(default = "default_preset_volume")]
    pub volume: f32,
}

impl RigPreset {
    /// Build a preset from a legacy standalone preset's processing blocks
    /// and volume. Blocks are kept bit-identical and in order, volume is
    /// preserved exact, and the preset has no scenes/scene-params — so it
    /// behaves as a single Default scene that changes nothing. Audio is
    /// identical to the legacy preset (CLAUDE.md invariant).
    pub fn from_legacy_blocks(blocks: Vec<AudioBlock>, volume: f32) -> Self {
        Self {
            blocks,
            scene_params: Vec::new(),
            scenes: BTreeMap::new(),
            volume,
        }
    }

    /// How many scenes this preset exposes. A preset starts with a
    /// single implicit scene (1); the user adds 2, 3… on demand. The
    /// count is the highest defined index (≥ 1) — scenes are a dense
    /// `1..=scene_count` range from the UI's point of view.
    pub fn scene_count(&self) -> usize {
        self.scenes.keys().max().copied().unwrap_or(1).max(1)
    }

    /// Chain volume % for `idx`: the scene's own override, else the
    /// preset volume (back-compat: scenes without an override are
    /// audibly identical to the legacy single-volume preset).
    pub fn scene_volume(&self, idx: usize) -> f32 {
        self.scenes
            .get(&idx)
            .and_then(|s| s.volume)
            .unwrap_or(self.volume)
    }

    /// The scene for `idx`, or an empty Default scene when this preset has no
    /// scenes (backward-compat: a pre-scenes preset behaves as one Default
    /// scene that changes nothing).
    pub fn scene_or_default(&self, idx: usize) -> RigScene {
        if self.scenes.is_empty() && idx == 1 {
            return RigScene::default();
        }
        self.scenes.get(&idx).cloned().unwrap_or_default()
    }

    /// Resolve scene `idx` into concrete blocks: clone the base blocks, apply
    /// the scene's bypass (`enabled = !bypassed`) and override **only** the
    /// marked `scene_params` with the scene's values. Anything not marked is
    /// fixed by the preset (Helix Snapshot rule). Pure & deterministic.
    pub fn apply_scene(&self, idx: usize) -> Vec<AudioBlock> {
        let scene = self.scene_or_default(idx);
        let mut blocks = self.blocks.clone();
        for block in &mut blocks {
            let bid = block.id.0.clone();
            if let Some(&bypassed) = scene.bypass.get(&bid) {
                block.enabled = !bypassed;
            }
            let params = match &mut block.kind {
                AudioBlockKind::Core(c) => Some(&mut c.params),
                AudioBlockKind::Nam(n) => Some(&mut n.params),
                _ => None,
            };
            if let Some(params) = params {
                let prefix = format!("{bid}.");
                for key in &self.scene_params {
                    if let Some(param_id) = key.strip_prefix(&prefix) {
                        if let Some(&value) = scene.params.get(key) {
                            params.insert(param_id.to_string(), ParameterValue::Float(value));
                        }
                    }
                }
            }
        }
        blocks
    }
}

impl RigProject {
    /// Persist a block/param edit made on the projected synthetic chain
    /// back into the active preset, **per scene (snapshot semantics)**:
    /// the edit is captured into the input's *active scene* only, so each
    /// scene keeps its own values. `preset.blocks` stays the factory
    /// template; a float param / bypass that differs from the template is
    /// stored as that scene's override (and the key auto-marked as a
    /// scene-param so `apply_scene` applies it). A value back at the
    /// template clears the override. No-op if input/preset is unknown.
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
                    if let ParameterValue::Float(v) = val {
                        let key = format!("{bid}.{pid}");
                        if bp.get_f32(pid) != Some(*v) {
                            set_param.push((key, *v));
                        } else {
                            clear_param.push(key);
                        }
                    }
                }
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

    /// Add a new preset to `input`'s bank: takes the next free slot
    /// (max key + 1, or 1 for an empty bank), clones the currently active
    /// preset as a starting point (an **independent** snapshot — no shared
    /// state), gives it a unique name, and makes the new slot active.
    /// Returns the new slot, or `None` if the input is unknown.
    pub fn add_preset_to_input(&mut self, input: &str) -> Option<usize> {
        let ri = self.inputs.get(input)?;
        let slot = ri.bank.keys().max().map(|m| m + 1).unwrap_or(1);
        let template = ri
            .bank
            .get(&ri.active_preset)
            .and_then(|n| self.presets.get(n))
            .cloned()
            .unwrap_or_else(|| RigPreset::from_legacy_blocks(Vec::new(), 100.0));
        let name = self.unique_preset_name("New Preset");
        self.presets.insert(name.clone(), template);
        let ri = self.inputs.get_mut(input)?;
        ri.bank.insert(slot, name);
        ri.active_preset = slot;
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

    /// Validate cross-references and per-input source channel conflicts.
    ///
    /// Rules (closed in #436 / scoped by #449):
    /// 1. every `bank` value must name a preset in `presets`;
    /// 2. each input's `active_preset` must be a key in its own `bank`;
    /// 3. each input's `active_scene` ∈ `1..=8`;
    /// 4. no preset may contain an `Input`/`Output` block;
    /// 5. per-input source channel conflicts — reuses
    ///    [`InputBlock::validate_channel_conflicts`];
    /// 6. every `routing` target must name an `outputs` entry.
    ///
    /// Cross-input capture exclusivity is **not** validated here: a project
    /// may freely hold many inputs that share a `(device, channel)` tap
    /// (a library of alternative configs). The constraint that two inputs
    /// sharing a tap cannot be *active simultaneously* is enforced by the
    /// engine at runtime, not by the static model.
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
            InputBlock {
                model: "standard".to_string(),
                entries: input.sources.clone(),
            }
            .validate_channel_conflicts()
            .map_err(|e| format!("input '{name}': {e}"))?;
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

#[cfg(test)]
#[path = "rig_tests.rs"]
mod rig_tests;

#[cfg(test)]
#[path = "rig_scene_tests.rs"]
mod rig_scene_tests;
