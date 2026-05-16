//! `RigProject` → engine bridge (#451).
//!
//! The engine only understands the legacy [`Chain`]. Rather than teach the
//! audio thread a new model, each [`RigInput`] (with its active preset and
//! routed outputs) is projected onto **one synthetic [`Chain`]**, then fed
//! through the existing, proven `build_runtime_graph` / per-input-runtime
//! machinery. Isolation (#4) is already enforced there — one runtime per
//! input, distinct `ChainId` per input. Pure and hardware-free.

use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use crate::runtime_graph::RuntimeGraph;
use anyhow::{anyhow, Result};
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;
use project::rig::RigProject;
use std::collections::HashMap;

/// Project each input of a `RigProject` onto one synthetic legacy `Chain`:
/// `Input(sources)` → active-preset processing blocks → `Output(routing)`.
///
/// Deterministic, ordered by input name. Each chain gets a distinct
/// `ChainId` (`rig:<input-name>`) so the existing runtime graph keeps the
/// inputs in fully isolated runtimes (invariant #4). Inputs whose active
/// preset is absent are skipped (a validated `RigProject` never hits this).
pub fn rig_to_chains(rig: &RigProject) -> Vec<Chain> {
    let mut chains = Vec::with_capacity(rig.inputs.len());
    for (name, input) in &rig.inputs {
        let Some(preset_name) = input.bank.get(&input.active_preset) else {
            continue;
        };
        let Some(preset) = rig.presets.get(preset_name) else {
            continue;
        };

        let mut blocks = Vec::with_capacity(preset.blocks.len() + 2);
        blocks.push(AudioBlock {
            id: BlockId(format!("rig:{name}:in")),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: input.sources.clone(),
            }),
        });
        blocks.extend(preset.apply_scene(input.active_scene));
        let routed: Vec<_> = input
            .routing
            .iter()
            .filter_map(|t| rig.outputs.get(t))
            .map(|o| o.entry.clone())
            .collect();
        if !routed.is_empty() {
            blocks.push(AudioBlock {
                id: BlockId(format!("rig:{name}:out")),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".to_string(),
                    entries: routed,
                }),
            });
        }

        chains.push(Chain {
            id: ChainId(format!("rig:{name}")),
            description: input.label.clone().or_else(|| Some(name.clone())),
            instrument: block_core::DEFAULT_INSTRUMENT.to_string(),
            enabled: true,
            volume: 100.0,
            blocks,
        });
    }
    chains
}

/// Owns the N isolated input runtimes of a `RigProject`.
///
/// Transport-agnostic (no Slint, no cpal here) — the host wires the resulting
/// [`RuntimeGraph`] to its backend. One synthetic chain per input keeps every
/// input in its own `ChainRuntimeState` (invariant #4). A preset switch
/// rebuilds **only that input's** chain through the proven
/// `RuntimeGraph::upsert_chain` path: same I/O signature ⇒ in-place lock-free
/// update (the `Arc<ChainRuntimeState>` is preserved, build happens off the
/// brief swap lock), so the audio thread never blocks or reallocates.
pub struct RigRuntime {
    project: RigProject,
    graph: RuntimeGraph,
    sample_rate: f32,
}

impl RigRuntime {
    /// Validate the project and build one isolated runtime per input.
    pub fn build(project: RigProject, sample_rate: f32) -> Result<Self> {
        project
            .validate()
            .map_err(|e| anyhow!("invalid project.openrig: {e}"))?;
        let mut graph = RuntimeGraph {
            chains: HashMap::new(),
        };
        for chain in rig_to_chains(&project) {
            graph.upsert_chain(&chain, sample_rate, false, &[DEFAULT_ELASTIC_TARGET])?;
        }
        Ok(Self {
            project,
            graph,
            sample_rate,
        })
    }

    pub fn project(&self) -> &RigProject {
        &self.project
    }

    pub fn graph(&self) -> &RuntimeGraph {
        &self.graph
    }

    /// Switch the active preset of one input to bank slot `idx`.
    ///
    /// Rebuilds only that input's synthetic chain via `upsert_chain` — the
    /// other inputs' runtimes are untouched (isolation #4). With an unchanged
    /// I/O signature this is the in-place lock-free swap.
    pub fn switch_preset(&mut self, input: &str, idx: usize) -> Result<()> {
        let ri = self
            .project
            .inputs
            .get_mut(input)
            .ok_or_else(|| anyhow!("unknown input '{input}'"))?;
        if !ri.bank.contains_key(&idx) {
            return Err(anyhow!("input '{input}' has no bank slot {idx}"));
        }
        ri.active_preset = idx;

        let id = ChainId(format!("rig:{input}"));
        let chain = rig_to_chains(&self.project)
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| anyhow!("input '{input}' has no buildable chain"))?;
        self.graph
            .upsert_chain(&chain, self.sample_rate, false, &[DEFAULT_ELASTIC_TARGET])?;
        Ok(())
    }

    /// Switch the active scene of one input (`1..=8`).
    ///
    /// Same lock-free in-place path as [`Self::switch_preset`] — only that
    /// input's chain is rebuilt (new blocks from `RigPreset::apply_scene`),
    /// other inputs untouched (#4). The previous-scene tail spillover is the
    /// dedicated #454-T5 RT step.
    pub fn switch_scene(&mut self, input: &str, scene: usize) -> Result<()> {
        if !(1..=8).contains(&scene) {
            return Err(anyhow!(
                "scene {scene} out of range 1..=8 for input '{input}'"
            ));
        }
        let ri = self
            .project
            .inputs
            .get_mut(input)
            .ok_or_else(|| anyhow!("unknown input '{input}'"))?;
        ri.active_scene = scene;

        let id = ChainId(format!("rig:{input}"));
        let chain = rig_to_chains(&self.project)
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| anyhow!("input '{input}' has no buildable chain"))?;
        self.graph
            .upsert_chain(&chain, self.sample_rate, false, &[DEFAULT_ELASTIC_TARGET])?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "rig_runtime_tests.rs"]
mod tests;
