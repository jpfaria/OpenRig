//! `project.openrig` — project-level I/O + per-input preset banks (#436).
//!
//! Wraps the existing `InputEntry`/`OutputEntry`/`AudioBlock` model 1:1 — single
//! source of truth, zero duplication. The legacy chain-based
//! [`crate::project::Project`] is untouched; migration is #450.
//!
//! Scope of #449: model + parser + validation only. No engine wiring, no
//! migration, no UI, no scenes (those are #450/#451/#452/#453/#454).

use crate::block::{AudioBlock, AudioBlockKind, InputEntry, OutputEntry};
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn default_scene() -> usize {
    1
}

fn default_instrument() -> String {
    block_core::DEFAULT_INSTRUMENT.to_string()
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
    /// Project-level MIDI bindings (ADR 0003 / #499). `None` for pre-#499
    /// projects — they fall back to the system bindings file or the shipped
    /// default at resolve time. Travels with the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi: Option<crate::midi::RigProjectMidi>,
    /// User-defined order of projected chains by input name (no `rig:`
    /// prefix). Empty ⇒ alphabetical `inputs` iteration (default). Issue
    /// #502 / regression of #246. Persists under the kebab-case key
    /// `chain-order:` to match `active-preset`, `scene-params`, etc.
    #[serde(default, rename = "chain-order", skip_serializing_if = "Vec::is_empty")]
    pub chain_order: Vec<String>,
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
    /// The instrument type for this input chain (e.g. "electric_guitar",
    /// "acoustic_guitar"). Defaults to "electric_guitar" for backward
    /// compatibility with pre-#627 `.openrig` files that have no field.
    #[serde(default = "default_instrument")]
    pub instrument: String,
    /// I/O binding id that this input's capture block references (#716).
    /// Empty string means unbound (legacy / device-level entries only).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub io: String,
    /// Endpoint name within the I/O binding that this input block uses (#716).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint: String,
}

/// One project output. Maps 1:1 onto the existing [`OutputEntry`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(flatten)]
    pub entry: OutputEntry,
    /// I/O binding id that this output block references (#716).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub io: String,
    /// Endpoint name within the I/O binding (#716).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint: String,
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

/// #436: a human label for a preset whose `name` is absent (legacy
/// projects saved before the field). The original description was lost
/// to the slug pool key on migration, so the slug is the only source:
/// de-slug `-`/`_` to spaces and Title-Case each word
/// (`studio-clean-compressor` → `Studio Clean Compressor`). Single
/// source of truth — used by the select and the chain title alike.
pub fn humanize_preset_label(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    /// Stable identity of this pool entry (the bank references it by
    /// key; this mirrors it so a loaded preset keeps its id). Empty for
    /// pre-#436 docs (`#[serde(default)]`) — back-compat.
    #[serde(default)]
    pub id: String,
    /// Human description shown in the UI (the original chain
    /// description on migration, or the preset file's `name`). `None`
    /// ⇒ fall back to the id/key. Pre-#436 docs have no field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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
            id: String::new(),
            name: None,
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

#[cfg(test)]
#[path = "rig_tests.rs"]
mod rig_tests;

#[cfg(test)]
#[path = "rig_scene_tests.rs"]
mod rig_scene_tests;

#[cfg(test)]
#[path = "rig_midi_tests.rs"]
mod rig_midi_tests;
