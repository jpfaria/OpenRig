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
    /// Validate cross-references and per-input source channel conflicts.
    ///
    /// Rules (closed in #436 / scoped by #449):
    /// 1. every `bank` value must name a preset in `presets`;
    /// 2. each input's `active_preset` must be a key in its own `bank`;
    /// 3. each input's `active_scene` ∈ `1..=8`;
    /// 4. no preset may contain an `Input`/`Output` block;
    /// 5. per-input source channel conflicts — reuses
    ///    [`InputBlock::validate_channel_conflicts`];
    /// 6. every `routing` target must name an `outputs` entry;
    /// 7. a `(device, channel)` capture source belongs to **at most one**
    ///    input — two isolated runtimes must never share a capture tap
    ///    (CLAUDE.md isolation invariant #4).
    pub fn validate(&self) -> Result<(), String> {
        // Rule 7: cross-input capture exclusivity. BTreeMap ⇒ deterministic
        // iteration ⇒ deterministic error message.
        let mut claimed: std::collections::BTreeMap<(String, usize), String> =
            std::collections::BTreeMap::new();
        for (name, input) in &self.inputs {
            for entry in &input.sources {
                for &ch in &entry.channels {
                    let key = (entry.device_id.0.clone(), ch);
                    match claimed.get(&key) {
                        // Same input claiming it twice ⇒ a per-input conflict;
                        // leave the precise message to rule 5 below.
                        Some(owner) if owner == name => {}
                        Some(owner) => {
                            return Err(format!(
                                "input '{name}' source device '{}' channel {ch} is \
                                 already used by input '{owner}' (capture taps cannot \
                                 be shared between inputs)",
                                entry.device_id.0
                            ));
                        }
                        None => {
                            claimed.insert(key, name.clone());
                        }
                    }
                }
            }
        }
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
