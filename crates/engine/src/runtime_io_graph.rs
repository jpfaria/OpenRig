//! Registry-aware runtime graph build (issue #716).
//!
//! Setup-time only — never runs on the audio thread. Bridges the per-binding
//! stream resolution (`io_routing`) to the runtime-state assembly
//! (`runtime_graph::assemble_chain_runtime_state`). Lives in its own module so
//! the routing rule does not grow the `runtime_graph` god file (#276).
//!
//! A chain with bound I/O ports (non-empty `io`) is routed PER BINDING: each
//! `(input port, output port)` pair that shares a binding (with
//! `inputPos <= outputPos`) becomes its own isolated stream running only the
//! effect blocks between the ports, reading the input endpoint and writing the
//! output endpoint. Input of binding A can never reach output of binding B
//! (CLAUDE.md invariant #4). A chain whose ports are all legacy/unbound (`io`
//! empty) falls back to the existing `entries`-based path — byte-identical to
//! `build_runtime_graph`.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use project::block::OutputEntry;
use project::chain::Chain;
use project::project::Project;

use crate::io_routing::{chain_has_bound_ports, resolve_chain_streams};
use crate::runtime_graph::{
    assemble_chain_runtime_state, build_per_input_runtimes, chain_has_enabled_insert, RuntimeGraph,
};
use crate::runtime_segments::ChainSegment;
use crate::runtime_state::ChainRuntimeState;

/// Registry-aware graph build. Resolution happens here, off the audio thread;
/// the audio path is unchanged. Bound chains take the per-binding routing path;
/// legacy chains reuse `build_per_input_runtimes`.
pub fn build_io_runtime_graph(
    project: &Project,
    chain_sample_rates: &HashMap<ChainId, f32>,
    chain_elastic_targets: &HashMap<ChainId, Vec<usize>>,
    io_bindings: &[IoBinding],
) -> Result<RuntimeGraph> {
    let mut chains = HashMap::new();
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        let sample_rate = *chain_sample_rates
            .get(&chain.id)
            .ok_or_else(|| anyhow!("chain '{}' has no resolved runtime sample rate", chain.id.0))?;
        let default_targets: Vec<usize> = Vec::new();
        let elastic_targets = chain_elastic_targets
            .get(&chain.id)
            .unwrap_or(&default_targets);

        if chain_has_bound_ports(chain) {
            for (group, state) in
                build_bound_chain_runtimes(chain, io_bindings, sample_rate, elastic_targets)?
            {
                state.set_volume_pct(chain.volume);
                chains.insert((chain.id.clone(), group), Arc::new(state));
            }
        } else {
            for (group, state) in build_per_input_runtimes(chain, sample_rate, elastic_targets)? {
                state.set_volume_pct(chain.volume);
                chains.insert((chain.id.clone(), group), Arc::new(state));
            }
        }
    }
    Ok(RuntimeGraph { chains })
}

/// Assemble the isolated runtimes for a chain whose ports reference io
/// bindings — ONE `ChainRuntimeState` per INPUT port (CLAUDE.md invariant #4),
/// mirroring the legacy `build_per_input_runtimes` decomposition.
///
/// Each input port's runtime processes only that port's `(input, output)`
/// streams. Distinct input ports become distinct runtimes keyed
/// `(chain.id, group)`; their outputs to the same physical output endpoint sum
/// at the backend (`process_output_f32_mixed`) POST each runtime's own
/// limiter — byte-equivalent to the legacy backend sum, not a pre-limiter sum
/// in a shared route accumulator. The per-binding pairing already forbids
/// cross-binding bleed (input of A never reaches output of B).
///
/// All per-input runtimes share the SAME `eff_outputs` ordering so route
/// index `j` means the same physical output endpoint in every runtime — that
/// is what lets the backend output stream sum runtime `[0..N]` route `j` for
/// each output device.
///
/// Insert chains form a cross-cpal pipeline (input → send → return → output);
/// splitting them per input port would sever it, so they keep a single
/// runtime (group `0`) packing every segment — same exception the legacy
/// `group_segments_by_input` makes.
fn build_bound_chain_runtimes(
    chain: &Chain,
    io_bindings: &[IoBinding],
    sample_rate: f32,
    elastic_targets: &[usize],
) -> Result<Vec<(usize, ChainRuntimeState)>> {
    let streams = resolve_chain_streams(chain, io_bindings);

    // One route per DISTINCT output port, first-seen order — shared by every
    // per-input runtime so route indices line up for the backend output mix.
    let mut eff_outputs: Vec<OutputEntry> = Vec::new();
    let mut output_route_of: HashMap<(String, String), usize> = HashMap::new();
    for s in &streams {
        let key = (s.output_binding.clone(), s.output_endpoint.clone());
        let next = eff_outputs.len();
        output_route_of.entry(key).or_insert_with(|| {
            eff_outputs.push(s.output_entry.clone());
            next
        });
    }

    // One cpal stream per DISTINCT input port, first-seen order. The cpal
    // index is ALSO the per-input group id — distinct ports become distinct
    // isolated runtimes; segments sharing a port dispatch on the same index.
    let mut input_group_of: HashMap<(String, String), usize> = HashMap::new();
    let mut next_group = 0usize;

    // Per input-port group, the segments that port owns. `order` keeps the
    // first-seen group sequence stable across rebuilds.
    let mut order: Vec<usize> = Vec::new();
    let mut groups: HashMap<usize, Vec<ChainSegment>> = HashMap::new();
    for s in &streams {
        let in_key = (s.input_binding.clone(), s.input_endpoint.clone());
        let group = *input_group_of.entry(in_key).or_insert_with(|| {
            let idx = next_group;
            next_group += 1;
            idx
        });
        if !groups.contains_key(&group) {
            order.push(group);
        }
        let route_idx = output_route_of[&(s.output_binding.clone(), s.output_endpoint.clone())];
        groups.entry(group).or_default().push(ChainSegment {
            input: s.input_entry.clone(),
            cpal_input_index: group,
            block_indices: s.block_indices.clone(),
            output_route_indices: vec![route_idx],
            split_mono_sibling_count: None,
            entry_group: group,
        });
    }

    // Insert chains: one runtime packing every segment (pipeline integrity).
    if chain_has_enabled_insert(chain) {
        let all_segments: Vec<ChainSegment> = order
            .iter()
            .flat_map(|g| groups.remove(g).unwrap_or_default())
            .collect();
        let state = assemble_chain_runtime_state(
            chain,
            &all_segments,
            &eff_outputs,
            sample_rate,
            elastic_targets,
            None,
        )?;
        return Ok(vec![(0, state)]);
    }

    // No-Insert: one isolated runtime per input port.
    let mut out = Vec::with_capacity(order.len());
    for group in order {
        let segments = groups.remove(&group).unwrap_or_default();
        let cpal_input_index = segments
            .first()
            .map(|s| s.cpal_input_index)
            .unwrap_or(group);
        let mut state = assemble_chain_runtime_state(
            chain,
            &segments,
            &eff_outputs,
            sample_rate,
            elastic_targets,
            None,
        )?;
        state.owned_entry = Some((group, cpal_input_index));
        out.push((group, state));
    }
    Ok(out)
}
