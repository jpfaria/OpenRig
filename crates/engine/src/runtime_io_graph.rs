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
//! (CLAUDE.md invariant #4). Clean break (#716): routing is binding-only — a
//! chain whose ports are all unbound (`io` empty) produces NO runtime. There is
//! no production fallback to the legacy `entries`-based path; a legacy project
//! opens UNBOUND and must be reconfigured via the registry.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use project::block::OutputEntry;
use project::chain::Chain;
use project::project::Project;

use crate::io_routing::{chain_has_bound_ports, resolve_chain_streams};
use crate::runtime_graph::{assemble_chain_runtime_state, chain_has_enabled_insert, RuntimeGraph};
use crate::runtime_segments::ChainSegment;
use crate::runtime_state::ChainRuntimeState;

/// Registry-aware graph build. Resolution happens here, off the audio thread;
/// the audio path is unchanged. Bound chains take the per-binding routing path;
/// unbound chains produce no runtime (binding-only routing, #716).
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

        // Clean break (#716): routing is binding-only. A chain whose ports are
        // all unbound (`io` empty) produces NO runtime — the user reconfigures
        // it via the registry. No production fallback to the legacy
        // `entries`-based all-to-all path.
        if !chain_has_bound_ports(chain) {
            continue;
        }
        for (group, state) in
            build_bound_chain_runtimes(chain, io_bindings, sample_rate, elastic_targets)?
        {
            state.set_volume_pct(chain.volume);
            chains.insert((chain.id.clone(), group), Arc::new(state));
        }
    }
    Ok(RuntimeGraph { chains })
}

impl RuntimeGraph {
    /// Issue #716 — binding-aware full rebuild for the synchronous live path
    /// (multi-device chains, JACK, live edits). Drops every stale per-input
    /// runtime for `chain` and rebuilds them via the per-binding router, then
    /// returns the first (lowest-group) runtime.
    ///
    /// This is the bound-chain twin of [`RuntimeGraph::upsert_chain`]. The
    /// legacy in-place fast path is `entries`-based, so it cannot serve a
    /// bound chain whose `entries` are drained — the controller dispatches
    /// bound chains here instead. A full rebuild on every edit is the same
    /// cost the legacy path already pays on a topology change.
    ///
    /// # Errors
    /// Propagates any failure from chain-runtime assembly, or if the bound
    /// chain produced no runtimes (e.g. every port failed to resolve against
    /// `io_bindings`).
    pub fn upsert_bound_chain(
        &mut self,
        chain: &Chain,
        sample_rate: f32,
        elastic_targets: &[usize],
        io_bindings: &[IoBinding],
    ) -> Result<Arc<ChainRuntimeState>> {
        let existing_groups: Vec<usize> = self
            .chains
            .keys()
            .filter(|(cid, _)| cid == &chain.id)
            .map(|(_, g)| *g)
            .collect();
        for g in &existing_groups {
            self.chains.remove(&(chain.id.clone(), *g));
        }
        let mut first: Option<Arc<ChainRuntimeState>> = None;
        for (group, state) in
            build_bound_chain_runtimes(chain, io_bindings, sample_rate, elastic_targets)?
        {
            state.set_volume_pct(chain.volume);
            let arc = Arc::new(state);
            if first.is_none() {
                first = Some(arc.clone());
            }
            self.chains.insert((chain.id.clone(), group), arc);
        }
        first.ok_or_else(|| {
            anyhow!(
                "bound chain '{}' produced no input runtimes (no port resolved against the registry)",
                chain.id.0
            )
        })
    }
}

/// Per-chain registry-aware build seam for the live audio path (issue #716).
///
/// This is the binding-aware twin of
/// [`build_per_input_runtime_states`](crate::runtime_graph::build_per_input_runtime_states):
/// infra-cpal's worker (`build_chain_runtime`) calls THIS so the per-binding
/// routing engine runs in the running app, not only in tests.
///
/// - A chain with bound ports (any Input/Output block carrying a non-empty
///   `io`) is routed PER BINDING via `resolve_chain_streams`: endpoints are
///   resolved from `io_bindings`, cross-binding isolation is structural.
/// - A chain whose ports are all unbound (`io` empty) produces NO runtime —
///   routing is binding-only (#716); a legacy project opens unbound.
///
/// Returns one `(group, Arc<ChainRuntimeState>)` per isolated input runtime,
/// the same shape the controller publishes into its `(chain, group)` slots.
///
/// # Errors
/// Propagates any failure from chain-runtime assembly (e.g. a model that fails
/// to load).
pub fn build_per_input_runtime_states_with_bindings(
    chain: &Chain,
    sample_rate: f32,
    elastic_targets: &[usize],
    io_bindings: &[IoBinding],
) -> Result<Vec<(usize, Arc<ChainRuntimeState>)>> {
    // #716 discovery: expand a chain that SELECTS bindings into its bound
    // Input/Output blocks (head/tail) so the whole build — bound-port detection
    // AND block execution — sees the synthesised I/O. Legacy bound chains
    // (blocks carry `io`, select nothing) and unbound chains pass through.
    let expanded = project::binding_discovery::resolve_bound_io_blocks(chain, io_bindings);
    let chain = &expanded;

    // Clean break (#716): routing is binding-only. An unbound chain (`io`
    // empty on every port) produces NO runtime — no fallback to the legacy
    // `entries` all-to-all path. The user reconfigures it via the registry.
    let built = if chain_has_bound_ports(chain) {
        build_bound_chain_runtimes(chain, io_bindings, sample_rate, elastic_targets)?
    } else {
        Vec::new()
    };
    Ok(built
        .into_iter()
        .map(|(group, state)| {
            // Mirror the volume seeding the graph builders do so a runtime
            // built through this seam matches one built via the graph path.
            state.set_volume_pct(chain.volume);
            (group, Arc::new(state))
        })
        .collect())
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
