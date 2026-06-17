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
use crate::runtime_graph::{assemble_chain_runtime_state, build_per_input_runtimes, RuntimeGraph};
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
            let state =
                build_bound_chain_runtime(chain, io_bindings, sample_rate, elastic_targets)?;
            state.set_volume_pct(chain.volume);
            chains.insert((chain.id.clone(), 0usize), Arc::new(state));
        } else {
            for (group, state) in build_per_input_runtimes(chain, sample_rate, elastic_targets)? {
                state.set_volume_pct(chain.volume);
                chains.insert((chain.id.clone(), group), Arc::new(state));
            }
        }
    }
    Ok(RuntimeGraph { chains })
}

/// Assemble ONE isolated runtime for a chain whose ports reference io bindings.
/// Each resolved per-binding stream becomes a segment routing ONLY to its own
/// binding's output route — that single-output routing is what blocks the
/// cross-binding bleed the chain-shared cartesian path produced. Distinct input
/// ports dispatch on distinct cpal indices, so feeding one input's callback
/// never drives another input's stream.
fn build_bound_chain_runtime(
    chain: &Chain,
    io_bindings: &[IoBinding],
    sample_rate: f32,
    elastic_targets: &[usize],
) -> Result<ChainRuntimeState> {
    let streams = resolve_chain_streams(chain, io_bindings);

    // One route per DISTINCT output port, first-seen order. Two streams to the
    // same output endpoint share a route (summed at the route — never across
    // bindings, which the resolver already forbids).
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

    // One cpal stream per DISTINCT input port, first-seen order. Segments
    // sharing an input dispatch on the same cpal index.
    let mut input_cpal_of: HashMap<(String, String), usize> = HashMap::new();
    let mut next_cpal = 0usize;

    let mut segments: Vec<ChainSegment> = Vec::with_capacity(streams.len());
    for s in &streams {
        let in_key = (s.input_binding.clone(), s.input_endpoint.clone());
        let cpal_idx = *input_cpal_of.entry(in_key).or_insert_with(|| {
            let idx = next_cpal;
            next_cpal += 1;
            idx
        });
        let route_idx = output_route_of[&(s.output_binding.clone(), s.output_endpoint.clone())];
        segments.push(ChainSegment {
            input: s.input_entry.clone(),
            cpal_input_index: cpal_idx,
            block_indices: s.block_indices.clone(),
            output_route_indices: vec![route_idx],
            split_mono_sibling_count: None,
            entry_group: cpal_idx,
        });
    }

    assemble_chain_runtime_state(
        chain,
        &segments,
        &eff_outputs,
        sample_rate,
        elastic_targets,
        None,
    )
}
