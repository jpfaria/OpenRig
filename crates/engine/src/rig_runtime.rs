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
use project::rig::{RigInput, RigProject};
use std::collections::{BTreeSet, HashMap};

/// The `(device, channel)` capture taps an input occupies. Two inputs are
/// in conflict iff their tap sets intersect — they would read the same
/// physical capture point, which two isolated runtimes must never share
/// (invariant #4).
fn input_taps(input: &RigInput) -> Vec<(String, usize)> {
    let mut taps = Vec::new();
    for entry in &input.sources {
        for &ch in &entry.channels {
            taps.push((entry.device_id.0.clone(), ch));
        }
    }
    taps
}

/// First tap of `candidate` already claimed by a currently-enabled input,
/// if any: `(device, channel, holder-input-name)`. Deterministic via the
/// project's `BTreeMap` ordering.
fn tap_conflict(
    project: &RigProject,
    enabled: &BTreeSet<String>,
    candidate: &RigInput,
) -> Option<(String, usize, String)> {
    let want = input_taps(candidate);
    for name in enabled {
        if let Some(other) = project.inputs.get(name) {
            for (dev, ch) in input_taps(other) {
                if want.iter().any(|(d, c)| *d == dev && *c == ch) {
                    return Some((dev, ch, name.clone()));
                }
            }
        }
    }
    None
}

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
            // Invariant #10: carry the preset's volume (legacy migration
            // preserved Chain.volume → RigPreset.volume). Hardcoding 100
            // would silently retune every preset on the rig path. The
            // active scene may override it (#436); a scene with no
            // override resolves to `preset.volume` ⇒ audibly unchanged
            // for every pre-#436 project (back-compat).
            volume: preset.scene_volume(input.active_scene),
            blocks,
        });
    }
    chains
}

/// Project a `RigProject` onto a synthetic **legacy** [`Project`]: **every**
/// input becomes a `Chain` so the existing GUI shows them all, and each
/// chain's `enabled` flag reflects whether that input is in `enabled`.
/// Enabling is the USER's action (in memory, at runtime) — nothing is
/// auto-started; pass an empty set to load everything OFF. Drives the
/// proven cpal/runtime path with zero new audio code; `device_settings`
/// is empty (per-machine settings live elsewhere).
pub fn rig_to_legacy_project(
    rig: &RigProject,
    enabled: &std::collections::BTreeSet<String>,
) -> project::project::Project {
    let chains = rig_to_chains(rig)
        .into_iter()
        .map(|mut c| {
            let on =
                c.id.0
                    .strip_prefix("rig:")
                    .is_some_and(|name| enabled.contains(name));
            c.enabled = on;
            c
        })
        .collect();
    project::project::Project {
        name: rig.name.clone(),
        device_settings: Vec::new(),
        chains,
    }
}

/// Apply a preset and/or scene change to one input of `rig` **in place**
/// and return that input's freshly-projected synthetic [`Chain`] (the
/// caller upserts it through the proven runtime path — zero new audio
/// code). `preset_slot`/`scene` are applied only when `Some`. Invalid
/// (unknown input, bank slot absent, scene ∉ `1..=8`) ⇒ **no mutation**
/// and `None`, so the GUI can ignore a bad request without corrupting
/// state. `None` is also returned if the resulting preset is unbuildable.
pub fn switch_and_project_input(
    rig: &mut RigProject,
    input: &str,
    preset_slot: Option<usize>,
    scene: Option<usize>,
) -> Option<Chain> {
    {
        // Validate everything before touching state (no partial mutation).
        let ri = rig.inputs.get(input)?;
        if let Some(s) = preset_slot {
            if !ri.bank.contains_key(&s) {
                return None;
            }
        }
        if let Some(sc) = scene {
            if !(1..=8).contains(&sc) {
                return None;
            }
        }
    }
    let ri = rig.inputs.get_mut(input)?;
    if let Some(s) = preset_slot {
        ri.active_preset = s;
    }
    if let Some(sc) = scene {
        ri.active_scene = sc;
    }
    let id = ChainId(format!("rig:{input}"));
    rig_to_chains(rig).into_iter().find(|c| c.id == id)
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
    /// Inputs currently activated, **in memory only** — never persisted to
    /// `project.openrig`. A tap-sharing input can only be enabled if no
    /// already-enabled input holds the same `(device, channel)`.
    enabled: BTreeSet<String>,
}

impl RigRuntime {
    /// Validate the project and bring up one isolated runtime per input,
    /// **skipping** any input whose `(device, channel)` tap is already held
    /// by an earlier-enabled input (deterministic by input name). Enabled
    /// state lives only here, never in the file; conflicting inputs stay
    /// defined but inactive and can be enabled later via [`Self::enable_input`]
    /// once the tap is freed.
    pub fn build(project: RigProject, sample_rate: f32) -> Result<Self> {
        project
            .validate()
            .map_err(|e| anyhow!("invalid project.openrig: {e}"))?;
        let mut graph = RuntimeGraph {
            chains: HashMap::new(),
        };
        let mut enabled = BTreeSet::new();
        for (name, input) in &project.inputs {
            if tap_conflict(&project, &enabled, input).is_some() {
                continue; // tap already in use ⇒ leave this input inactive
            }
            let id = ChainId(format!("rig:{name}"));
            if let Some(chain) = rig_to_chains(&project).into_iter().find(|c| c.id == id) {
                graph.upsert_chain(&chain, sample_rate, false, &[DEFAULT_ELASTIC_TARGET])?;
                enabled.insert(name.clone());
            }
        }
        Ok(Self {
            project,
            graph,
            sample_rate,
            enabled,
        })
    }

    /// Is this input currently activated (in-memory)?
    pub fn is_enabled(&self, input: &str) -> bool {
        self.enabled.contains(input)
    }

    /// Activate an input at runtime. Fails if the input is unknown or any of
    /// its `(device, channel)` taps is already held by an enabled input
    /// (disable that one first). No-op if already enabled.
    pub fn enable_input(&mut self, input: &str) -> Result<()> {
        let ri = self
            .project
            .inputs
            .get(input)
            .ok_or_else(|| anyhow!("unknown input '{input}'"))?;
        if self.enabled.contains(input) {
            return Ok(());
        }
        if let Some((dev, ch, holder)) = tap_conflict(&self.project, &self.enabled, ri) {
            return Err(anyhow!(
                "cannot enable input '{input}': device '{dev}' channel {ch} \
                 is already in use by active input '{holder}'"
            ));
        }
        let id = ChainId(format!("rig:{input}"));
        let chain = rig_to_chains(&self.project)
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| anyhow!("input '{input}' has no buildable chain"))?;
        self.graph
            .upsert_chain(&chain, self.sample_rate, false, &[DEFAULT_ELASTIC_TARGET])?;
        self.enabled.insert(input.to_string());
        Ok(())
    }

    /// Deactivate an input at runtime, tearing down its isolated runtime and
    /// freeing its capture taps for another input. Fails if unknown; no-op
    /// if already disabled.
    pub fn disable_input(&mut self, input: &str) -> Result<()> {
        if !self.project.inputs.contains_key(input) {
            return Err(anyhow!("unknown input '{input}'"));
        }
        if self.enabled.remove(input) {
            self.graph.remove_chain(&ChainId(format!("rig:{input}")));
        }
        Ok(())
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
        if !self.enabled.contains(input) {
            return Err(anyhow!(
                "input '{input}' is not active; enable it before switching presets"
            ));
        }
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
        if !self.enabled.contains(input) {
            return Err(anyhow!(
                "input '{input}' is not active; enable it before switching scenes"
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
