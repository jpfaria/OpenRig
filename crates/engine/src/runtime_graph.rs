//! Audio runtime graph construction (slice 3 of Phase 2 issue #194).
//!
//! Setup-time only — every function in this module runs when a chain is
//! built or rebuilt, never on the audio thread. The hot path lives in
//! `runtime.rs`. Extracting these here reduces `runtime.rs` from
//! ~2577 LOC toward the < 600 cap and isolates the bulk of the graph
//! construction logic from the audio thread code that consumes it.
//!
//! What's here:
//!   - `RuntimeGraph` (the project-level container) + its impl.
//!   - `build_runtime_graph` / `build_chain_runtime_state` /
//!     `update_chain_runtime_state` — entry points called by infra-cpal
//!     and adapter-console when a chain is added, swapped, or removed.
//!   - Chain segmentation (`split_chain_into_segments`, effective
//!     I/O resolution, insert teardown of the chain into per-input
//!     pipelines).
//!   - Block-level node construction (`build_block_runtime_node`,
//!     `build_core_block_runtime_node`, `build_select_runtime_node`,
//!     `bypass_runtime_node`, `build_audio_processor_for_model`).
//!   - Block-instance serial generator (`next_block_instance_serial`).
//!
//! What's NOT here:
//!   - Anything that runs per audio callback. That stays in
//!     `runtime.rs`.
//!   - The `ChainRuntimeState` struct itself — still in `runtime.rs`
//!     for now (slice 4 will move it). All this module does is
//!     CONSTRUCT it; the field accesses go through the (re-exported)
//!     pub(crate) fields.
//!
//! Re-exports back to `runtime`:
//!   - `RuntimeGraph`, `build_runtime_graph`, `build_chain_runtime_state`,
//!     `update_chain_runtime_state` are all `pub use`'d from `runtime`
//!     so the existing `engine::runtime::*` paths used by infra-cpal /
//!     adapter-console keep working unchanged.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::IoBinding;
use project::block::AudioBlockKind;
use project::chain::Chain;
use project::project::Project;

use crate::runtime::ChainRuntimeState;
use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_segments::{split_chain_into_segments, ChainSegment};

/// Bounded capacity for the per-chain SPSC error queue. Audio-thread
/// errors are dropped silently when the queue is full; the UI drains
/// every 200 ms, so 64 slots covers ~13 s of one-error-per-frame at
/// 48 kHz / 64-frame buffers.
pub(crate) const ERROR_QUEUE_CAPACITY: usize = 64;

/// Issue #350 — stream isolation invariant (CLAUDE.md #4).
///
/// One `ChainRuntimeState` per effective input runtime, NOT one per YAML
/// chain. The key is `(ChainId, input_group)` where `input_group` is the
/// CPAL stream index of the segments that runtime owns. Two InputBlocks
/// on two physical devices in the same YAML chain therefore produce TWO
/// fully isolated runtimes — each with its own `output_routes`
/// (`OutputRoutingState` + `ElasticBuffer`), its own `input_taps`, and
/// its own `processing` Mutex. No buffer/lock/route/tap is shared across
/// streams; mixing into one physical device happens at the cpal/JACK
/// backend, never by two producers on one of our SPSC rings.
///
/// `.len()` = total isolated streams. `.values()` = every per-input
/// runtime. Single-input chains produce exactly one entry
/// `(chain.id, group)` and are byte-identical to the pre-#350 behaviour
/// (same segments, same routes — the audio path is unchanged).
pub struct RuntimeGraph {
    pub chains: HashMap<(ChainId, usize), Arc<ChainRuntimeState>>,
}

/// Whether the chain has at least one enabled Insert block. Insert chains
/// form a cross-cpal-index pipeline (input → insert send → insert return →
/// output); splitting them by cpal index would sever the pipeline. Phase 1
/// keeps Insert chains as a single runtime (byte-identical to pre-#350);
/// the structural per-input isolation targets the no-Insert multi-input
/// case (the user-visible "two guitars, one chain" scenario).
pub(crate) fn chain_has_enabled_insert(chain: &Chain) -> bool {
    chain
        .blocks
        .iter()
        .any(|b| b.enabled && matches!(&b.kind, AudioBlockKind::Insert(_)))
}

/// Partition a chain's segments into per-RAW-input-entry groups (issue
/// #703). Each group becomes one isolated `ChainRuntimeState`. The group
/// id is the raw `InputEntry` index the segments came from: two entries
/// reading the SAME physical device are still two isolated runtimes (the
/// device's single cpal stream fans out to all of them), while split-mono
/// siblings (one raw entry, `mode: mono, channels: [a, b]`) share a group
/// — separating them would double-limit the pinned g02/g03 sum.
///
/// Linux/JACK keeps the per-device (cpal index) grouping behind a cfg
/// guard: the JACK-direct client binds ONE runtime, so a per-entry split
/// would silence every entry but the first there. Cross-platform law —
/// the cpal platforms' isolation gain must not change JACK behaviour.
///
/// Insert chains are NOT partitioned (single group `0`) — see
/// `chain_has_enabled_insert`.
pub(crate) fn group_segments_by_input(
    chain: &Chain,
    segments: Vec<ChainSegment>,
) -> Vec<(usize, Vec<ChainSegment>)> {
    if chain_has_enabled_insert(chain) || segments.is_empty() {
        return vec![(0, segments)];
    }
    #[cfg(all(target_os = "linux", feature = "jack"))]
    let key_of = |seg: &ChainSegment| seg.cpal_input_index;
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    let key_of = |seg: &ChainSegment| seg.entry_group;
    // Preserve first-seen order of group keys so runtime 0 is the first
    // input, runtime 1 the second, etc. (stable across rebuilds).
    let mut order: Vec<usize> = Vec::new();
    let mut groups: HashMap<usize, Vec<ChainSegment>> = HashMap::new();
    for seg in segments {
        let key = key_of(&seg);
        if !groups.contains_key(&key) {
            order.push(key);
        }
        groups.entry(key).or_default().push(seg);
    }
    order
        .into_iter()
        .map(|k| {
            let segs = groups.remove(&k).unwrap_or_default();
            (k, segs)
        })
        .collect()
}

pub fn build_runtime_graph(
    project: &Project,
    chain_sample_rates: &HashMap<ChainId, f32>,
    chain_elastic_targets: &HashMap<ChainId, Vec<usize>>,
    registry: &[IoBinding],
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
        let no_device_rates: HashMap<DeviceId, f32> = HashMap::new();
        for (group, state) in build_per_input_runtimes(
            chain,
            sample_rate,
            &no_device_rates,
            elastic_targets,
            registry,
        )? {
            state.set_volume_pct(chain.volume);
            chains.insert((chain.id.clone(), group), Arc::new(state));
        }
    }
    Ok(RuntimeGraph { chains })
}

/// Build one `ChainRuntimeState` per effective-input group of the chain.
/// Returns `(group_id, state)` pairs. For single-input / Insert chains
/// this is exactly one pair whose state equals the legacy
/// `build_chain_runtime_state` output (byte-identical audio path).
pub(crate) fn build_per_input_runtimes(
    chain: &Chain,
    sample_rate: f32,
    device_rates: &HashMap<DeviceId, f32>,
    elastic_targets: &[usize],
    registry: &[IoBinding],
) -> Result<Vec<(usize, ChainRuntimeState)>> {
    let (resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let (eff_inputs, eff_input_cpal_indices, eff_split_positions, eff_entry_groups) =
        effective_inputs(chain, &resolved_inputs, registry);
    let eff_outputs = effective_outputs(chain, &resolved_outputs, registry);
    let all_segments = split_chain_into_segments(
        chain,
        &eff_inputs,
        &eff_input_cpal_indices,
        &eff_split_positions,
        &eff_entry_groups,
        &eff_outputs,
        registry,
    );
    let groups = group_segments_by_input(chain, all_segments);
    let mut out = Vec::with_capacity(groups.len());
    for (group, segments) in groups {
        // All segments of a group share one effective input, hence one
        // cpal stream — record it so infra-cpal can fan a shared device
        // callback out to every per-entry runtime it feeds (issue #703).
        let cpal_input_index = segments.first().map(|s| s.cpal_input_index).unwrap_or(0);
        // #736: this group is one isolated input → one device → its OWN rate.
        // Empty/absent override falls back to the chain scalar (bit-identical
        // single-binding behaviour). Within a binding, input rate == output
        // rate is validated at resolve time, so the input device's rate is the
        // whole stream's rate.
        let group_rate = segments
            .first()
            .and_then(|s| device_rates.get(&s.input.device_id).copied())
            .unwrap_or(sample_rate);
        let mut state = assemble_chain_runtime_state(
            chain,
            &segments,
            &eff_outputs,
            group_rate,
            elastic_targets,
            None,
        )?;
        state.owned_entry = Some((group, cpal_input_index));
        out.push((group, state));
    }
    Ok(out)
}

/// Issue #703: public seam for infra layers that build a chain's runtimes
/// off-thread (cold activation, live rebuild). One isolated
/// `ChainRuntimeState` per input-entry group — the same shape
/// `RuntimeGraph` holds, so the caller can publish each runtime into its
/// `(chain, group)` slot. Single-entry chains return exactly one
/// `(0, state)` pair (the legacy whole-chain shape, byte-identical).
///
/// # Errors
/// Propagates any failure from chain-runtime assembly (e.g. a model that
/// fails to load).
pub fn build_per_input_runtime_states(
    chain: &Chain,
    sample_rate: f32,
    device_rates: &HashMap<DeviceId, f32>,
    elastic_targets: &[usize],
    registry: &[IoBinding],
) -> Result<Vec<(usize, Arc<ChainRuntimeState>)>> {
    Ok(
        build_per_input_runtimes(chain, sample_rate, device_rates, elastic_targets, registry)?
            .into_iter()
            .map(|(group, state)| (group, Arc::new(state)))
            .collect(),
    )
}

/// The per-input group ids a chain would produce, WITHOUT instantiating any
/// block processor. Issue #588: the in-place `upsert_chain` fast path used to
/// call `build_per_input_runtimes` purely to read these ids for a topology
/// comparison — which loaded every NAM/IR model in the chain from disk on
/// each edit, only to throw the runtime away. The grouping depends solely on
/// the chain's input/output endpoints and segment split, never on the built
/// processors, so it can be derived directly.
pub(crate) fn input_group_ids(chain: &Chain, registry: &[IoBinding]) -> Vec<usize> {
    let (resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let (eff_inputs, eff_input_cpal_indices, eff_split_positions, eff_entry_groups) =
        effective_inputs(chain, &resolved_inputs, registry);
    let eff_outputs = effective_outputs(chain, &resolved_outputs, registry);
    let all_segments = split_chain_into_segments(
        chain,
        &eff_inputs,
        &eff_input_cpal_indices,
        &eff_split_positions,
        &eff_entry_groups,
        &eff_outputs,
        registry,
    );
    group_segments_by_input(chain, all_segments)
        .into_iter()
        .map(|(group, _segments)| group)
        .collect()
}

pub fn build_chain_runtime_state(
    chain: &Chain,
    sample_rate: f32,
    elastic_targets: &[usize],
    registry: &[IoBinding],
) -> Result<ChainRuntimeState> {
    let (resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let (eff_inputs, eff_input_cpal_indices, eff_split_positions, eff_entry_groups) =
        effective_inputs(chain, &resolved_inputs, registry);
    let eff_outputs = effective_outputs(chain, &resolved_outputs, registry);
    log::info!("=== CHAIN '{}' RUNTIME BUILD ===", chain.id.0);
    log::info!("  inputs: {}", eff_inputs.len());
    for (i, inp) in eff_inputs.iter().enumerate() {
        log::info!(
            "    input[{}]: 'input #{}' dev='{}' ch={:?} cpal_stream={}",
            i,
            i,
            inp.device_id.0.split(':').next_back().unwrap_or("?"),
            inp.channels,
            eff_input_cpal_indices[i]
        );
    }
    log::info!("  outputs: {}", eff_outputs.len());
    for (i, out) in eff_outputs.iter().enumerate() {
        log::info!(
            "    output[{}]: 'output #{}' dev='{}' ch={:?}",
            i,
            i,
            out.device_id.0.split(':').next_back().unwrap_or("?"),
            out.channels
        );
    }
    let segments = split_chain_into_segments(
        chain,
        &eff_inputs,
        &eff_input_cpal_indices,
        &eff_split_positions,
        &eff_entry_groups,
        &eff_outputs,
        registry,
    );
    log::info!("  segments: {}", segments.len());
    for (i, seg) in segments.iter().enumerate() {
        let block_names: Vec<String> = seg
            .block_indices
            .iter()
            .filter_map(|&idx| chain.blocks.get(idx))
            .map(|b| {
                format!(
                    "{}({})",
                    b.id.0,
                    match &b.kind {
                        AudioBlockKind::Core(c) => c.effect_type.as_str(),
                        AudioBlockKind::Nam(_) => "nam",
                        _ => "?",
                    }
                )
            })
            .collect();
        log::info!(
            "    seg[{}]: input='input #{}' → blocks={:?} → output_routes={:?}",
            i,
            i,
            block_names,
            seg.output_route_indices
        );
    }
    log::info!("=== END CHAIN '{}' ===", chain.id.0);

    // Build the full single-runtime state (ALL segments). Probe / unit
    // tests / single-physical-device chains rely on this whole-chain
    // shape; per-input isolation for the multi-device case is composed in
    // `build_per_input_runtimes` which calls `assemble_chain_runtime_state`
    // per cpal-input group.
    assemble_chain_runtime_state(
        chain,
        &segments,
        &eff_outputs,
        sample_rate,
        elastic_targets,
        None,
    )
}

// Issue #792 split: chain-runtime assembly, the in-place rebuild path, and the
// `RuntimeGraph` methods live in sibling files. The graph entry points here call
// `assemble_chain_runtime_state`; the update fns are re-exported so the
// `engine::runtime_graph::*` and `engine::runtime::*` paths keep resolving
// unchanged for callers and tests.
use crate::runtime_graph_assemble::assemble_chain_runtime_state;
pub use crate::runtime_graph_update::{
    update_chain_runtime_state, update_chain_runtime_state_spillover,
};
#[cfg(test)]
pub(crate) use crate::runtime_graph_assemble::build_output_routing_state;

#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
#[path = "runtime_graph_issue_736_tests.rs"]
mod issue_736_per_binding_rate_tests;

#[cfg(test)]
#[path = "issue_592_elastic_prime_tests.rs"]
mod issue_592_elastic_prime_tests;
