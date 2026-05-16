//! `project.openrig` — project-level I/O + per-input preset banks (#436).
//!
//! Wraps the existing `InputEntry`/`OutputEntry`/`AudioBlock` model 1:1 — single
//! source of truth, zero duplication. The legacy chain-based
//! [`crate::project::Project`] is untouched; migration is #450.
//!
//! Scope of #449: model + parser + validation only. No engine wiring, no
//! migration, no UI, no scenes (those are #450/#451/#452/#453/#454).

use crate::block::{AudioBlockKind, InputBlock, InputEntry, OutputEntry};
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

/// A preset in the shared pool: processing chain only, no I/O.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigPreset {
    #[serde(default)]
    pub blocks: Vec<crate::block::AudioBlock>,
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
    /// 6. every `routing` target must name an `outputs` entry.
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
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "rig_tests.rs"]
mod rig_tests;
