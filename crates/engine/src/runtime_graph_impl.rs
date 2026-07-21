//! `RuntimeGraph` methods (issue #792 split from `runtime_graph.rs`).
//!
//! The container's add / swap / remove operations. `upsert_chain` takes the
//! fast in-place path (`update_chain_runtime_state`) when the per-input
//! topology is unchanged, else a full per-input rebuild. Setup-time only.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::IoBinding;
use project::chain::Chain;

use crate::runtime::ChainRuntimeState;
use crate::runtime_graph::{build_per_input_runtimes, input_group_ids, RuntimeGraph};
use crate::runtime_graph_update::{update_chain_runtime_state, update_chain_runtime_state_spillover};

impl RuntimeGraph {
    /// All per-input runtimes for a chain, ordered by group id (the cpal
    /// input index). For single-input / Insert chains this is a one-element
    /// vec. Issue #350: callers that fan a chain edit / teardown across
    /// every isolated stream iterate this.
    pub fn runtimes_for(&self, chain_id: &ChainId) -> Vec<Arc<ChainRuntimeState>> {
        self.runtimes_with_groups_for(chain_id)
            .into_iter()
            .map(|(_, rt)| rt)
            .collect()
    }

    /// Like [`runtimes_for`] but keeps the group id (the cpal input index
    /// the runtime owns) alongside each runtime, ordered by group. Issue
    /// #350 phase 3: the cpal layer needs the group id to bind each
    /// physical input device's stream to ITS OWN runtime `(chain, group)`.
    pub fn runtimes_with_groups_for(
        &self,
        chain_id: &ChainId,
    ) -> Vec<(usize, Arc<ChainRuntimeState>)> {
        let mut entries: Vec<(usize, Arc<ChainRuntimeState>)> = self
            .chains
            .iter()
            .filter(|((cid, _), _)| cid == chain_id)
            .map(|((_, g), rt)| (*g, rt.clone()))
            .collect();
        entries.sort_by_key(|(g, _)| *g);
        entries
    }

    pub fn upsert_chain(
        &mut self,
        chain: &Chain,
        sample_rate: f32,
        device_rates: &HashMap<DeviceId, f32>,
        reset_output_queue: bool,
        elastic_targets: &[usize],
        registry: &[IoBinding],
    ) -> Result<Arc<ChainRuntimeState>> {
        self.upsert_chain_impl(
            chain,
            sample_rate,
            device_rates,
            reset_output_queue,
            elastic_targets,
            false,
            registry,
        )
    }

    /// #454-T5: in-place swap that lets the previous preset/scene's tail
    /// ring out in parallel (spillover). Same lock-free guarantees.
    pub fn upsert_chain_spillover(
        &mut self,
        chain: &Chain,
        sample_rate: f32,
        device_rates: &HashMap<DeviceId, f32>,
        reset_output_queue: bool,
        elastic_targets: &[usize],
        registry: &[IoBinding],
    ) -> Result<Arc<ChainRuntimeState>> {
        self.upsert_chain_impl(
            chain,
            sample_rate,
            device_rates,
            reset_output_queue,
            elastic_targets,
            true,
            registry,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn upsert_chain_impl(
        &mut self,
        chain: &Chain,
        sample_rate: f32,
        device_rates: &HashMap<DeviceId, f32>,
        reset_output_queue: bool,
        elastic_targets: &[usize],
        spillover: bool,
        registry: &[IoBinding],
    ) -> Result<Arc<ChainRuntimeState>> {
        let existing_groups: Vec<usize> = self
            .chains
            .keys()
            .filter(|(cid, _)| cid == &chain.id)
            .map(|(_, g)| *g)
            .collect();

        // Fast in-place rebuild path: the per-input topology is UNCHANGED
        // (same set of group ids). Update every existing runtime in place
        // so the `Arc<ChainRuntimeState>` each live cpal callback captured
        // stays valid and observes the edit (volume, knob, block toggle).
        //
        // Issue #350 regression: the previous version only took this path
        // for single-input chains (`existing_groups.len() == 1`). For a
        // multi-input chain (e.g. 2 guitars on 2 devices) it fell through
        // to the full rebuild below, which drops the old Arcs and inserts
        // brand-new ones — but a volume/param edit does NOT rebuild the
        // cpal streams, so the callbacks kept the OLD Arcs and the edit
        // never reached the audio thread (slider did nothing).
        if !existing_groups.is_empty() {
            // Issue #588: derive the new group ids WITHOUT building runtimes —
            // building here reloaded every NAM/IR model in the chain from disk
            // on each edit (volume, knob, toggle), only to discard them and
            // update the existing runtime in place.
            let mut new_groups: Vec<usize> = input_group_ids(chain, registry);
            let mut existing_sorted = existing_groups.clone();
            new_groups.sort_unstable();
            existing_sorted.sort_unstable();
            if new_groups == existing_sorted {
                // Topology unchanged → in-place update of each existing
                // runtime, preserving the Arcs the callbacks hold.
                for group in &existing_sorted {
                    if let Some(runtime) = self.chains.get(&(chain.id.clone(), *group)) {
                        if spillover {
                            update_chain_runtime_state_spillover(
                                runtime,
                                chain,
                                sample_rate,
                                reset_output_queue,
                                elastic_targets,
                                registry,
                            )?;
                        } else {
                            update_chain_runtime_state(
                                runtime,
                                chain,
                                sample_rate,
                                reset_output_queue,
                                elastic_targets,
                                registry,
                            )?;
                        }
                    }
                }
                let first_group = existing_sorted[0];
                if let Some(rt) = self.chains.get(&(chain.id.clone(), first_group)) {
                    return Ok(rt.clone());
                }
            }
            // Topology changed (input added/removed/device swapped):
            // fall through to a full per-input rebuild (the stream
            // signature also changed, so the cpal streams WILL be rebuilt
            // and will capture the fresh Arcs).
        }

        // Full rebuild: drop every stale per-input runtime for this chain
        // and recreate one isolated runtime per effective input.
        for g in &existing_groups {
            self.chains.remove(&(chain.id.clone(), *g));
        }
        let mut first: Option<Arc<ChainRuntimeState>> = None;
        for (group, state) in
            build_per_input_runtimes(chain, sample_rate, device_rates, elastic_targets, registry)?
        {
            state.set_volume_pct(chain.volume);
            let arc = Arc::new(state);
            if first.is_none() {
                first = Some(arc.clone());
            }
            self.chains.insert((chain.id.clone(), group), arc);
        }
        first.ok_or_else(|| anyhow!("chain '{}' produced no input runtimes", chain.id.0))
    }

    pub fn remove_chain(&mut self, chain_id: &ChainId) {
        // Issue #350: a chain may own N per-input runtimes; drop them all.
        self.chains.retain(|(cid, _), _| cid != chain_id);
    }

    /// First (lowest-group) per-input runtime for a chain. Kept for
    /// callers that historically operated on "the chain's runtime"
    /// (latency probe arming, draining a single runtime). Multi-input
    /// fan-out for these call sites is Phase 3 (#350).
    pub fn runtime_for_chain(&self, chain_id: &ChainId) -> Option<Arc<ChainRuntimeState>> {
        self.runtimes_for(chain_id).into_iter().next()
    }
}
