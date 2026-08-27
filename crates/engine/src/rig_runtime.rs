//! Responsibility: turns a rig into the chains the engine runs.
//! `RigProject` → engine bridge (#451).
//!
//! The engine only understands the legacy [`Chain`]. Rather than teach the
//! audio thread a new model, each [`RigInput`] (with its active preset and
//! routed outputs) is projected onto **one synthetic [`Chain`]**, then fed
//! through the existing, proven `build_runtime_graph` / per-input-runtime
//! machinery. Isolation (#4) is already enforced there — one runtime per
//! input, distinct `ChainId` per input. Pure and hardware-free.

pub use crate::rig_projection::{rig_to_chains, rig_to_legacy_project, switch_and_project_input};
pub use crate::rig_runtime_normalize::looper_playback_blocks;
pub(crate) use crate::rig_tap_conflict::tap_conflict;
use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use crate::runtime_graph::RuntimeGraph;
use anyhow::{anyhow, Result};
use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use project::rig::RigProject;
use std::collections::{BTreeSet, HashMap};

// The test modules hang off this path and build blocks through `super::`,
// where these were in scope before the split (#873).
#[cfg(test)]
pub(crate) use domain::ids::BlockId;
#[cfg(test)]
pub(crate) use project::block::{AudioBlock, AudioBlockKind};
#[cfg(test)]
pub(crate) use project::chain::Chain;

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
    /// Per-machine I/O binding registry — the single source of device I/O.
    /// Resolved into chain ports at build/upsert time (model A, #716).
    registry: Vec<IoBinding>,
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
    pub fn build(project: RigProject, sample_rate: f32, registry: Vec<IoBinding>) -> Result<Self> {
        project
            .validate()
            .map_err(|e| anyhow!("invalid project.openrig: {e}"))?;
        let mut graph = RuntimeGraph {
            chains: HashMap::new(),
        };
        let mut enabled = BTreeSet::new();
        for (name, input) in &project.inputs {
            if tap_conflict(&project, &enabled, input, &registry).is_some() {
                continue; // tap already in use ⇒ leave this input inactive
            }
            let id = ChainId(format!("rig:{name}"));
            if let Some(chain) = rig_to_chains(&project).into_iter().find(|c| c.id == id) {
                graph.upsert_chain(
                    &chain,
                    sample_rate,
                    &HashMap::new(),
                    false,
                    &[DEFAULT_ELASTIC_TARGET],
                    &registry,
                )?;
                enabled.insert(name.clone());
            }
        }
        Ok(Self {
            project,
            graph,
            sample_rate,
            registry,
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
        if let Some((dev, ch, holder)) =
            tap_conflict(&self.project, &self.enabled, ri, &self.registry)
        {
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
        self.graph.upsert_chain(
            &chain,
            self.sample_rate,
            &HashMap::new(),
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &self.registry,
        )?;
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
        self.graph.upsert_chain(
            &chain,
            self.sample_rate,
            &HashMap::new(),
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &self.registry,
        )?;
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
        self.graph.upsert_chain(
            &chain,
            self.sample_rate,
            &HashMap::new(),
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &self.registry,
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "rig_runtime_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "rig_runtime_tests_scene.rs"]
mod tests_scene;

#[cfg(test)]
#[path = "rig_runtime_chain_order_tests.rs"]
mod chain_order_tests;

#[cfg(test)]
#[path = "rig_instrument_roundtrip_tests.rs"]
mod instrument_roundtrip_tests;
