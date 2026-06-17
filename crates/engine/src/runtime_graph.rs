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

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use arc_swap::ArcSwap;
use crossbeam_queue::ArrayQueue;

use block_core::{AudioChannelLayout, StreamHandle};
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlockKind, InputEntry, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMixdown, ChainOutputMode};
use project::project::Project;

use crate::runtime::{
    layout_label, ChainRuntimeState, DEFAULT_ELASTIC_TARGET, FADE_IN_FRAMES, PROBE_IDLE,
};
use crate::runtime_audio_frame::ElasticBuffer;
use crate::runtime_endpoints::{effective_inputs, effective_outputs};
use crate::runtime_segments::{split_chain_into_segments, ChainSegment};
use crate::runtime_state::{
    lock_recover, BlockRuntimeNode, ChainProcessingState, InputCallbackScratch,
    InputProcessingState, OutgoingTail, OutputRoutingState, RuntimeProcessor, SPILLOVER_FRAMES,
};

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
fn chain_has_enabled_insert(chain: &Chain) -> bool {
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
fn group_segments_by_input(
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
        for (group, state) in build_per_input_runtimes(chain, sample_rate, elastic_targets)? {
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
    elastic_targets: &[usize],
) -> Result<Vec<(usize, ChainRuntimeState)>> {
    let (eff_inputs, eff_input_cpal_indices, eff_split_positions, eff_entry_groups) =
        effective_inputs(chain);
    let eff_outputs = effective_outputs(chain);
    let all_segments = split_chain_into_segments(
        chain,
        &eff_inputs,
        &eff_input_cpal_indices,
        &eff_split_positions,
        &eff_entry_groups,
        &eff_outputs,
    );
    let groups = group_segments_by_input(chain, all_segments);
    let mut out = Vec::with_capacity(groups.len());
    for (group, segments) in groups {
        // All segments of a group share one effective input, hence one
        // cpal stream — record it so infra-cpal can fan a shared device
        // callback out to every per-entry runtime it feeds (issue #703).
        let cpal_input_index = segments.first().map(|s| s.cpal_input_index).unwrap_or(0);
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
    elastic_targets: &[usize],
) -> Result<Vec<(usize, Arc<ChainRuntimeState>)>> {
    Ok(
        build_per_input_runtimes(chain, sample_rate, elastic_targets)?
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
fn input_group_ids(chain: &Chain) -> Vec<usize> {
    let (eff_inputs, eff_input_cpal_indices, eff_split_positions, eff_entry_groups) =
        effective_inputs(chain);
    let eff_outputs = effective_outputs(chain);
    let all_segments = split_chain_into_segments(
        chain,
        &eff_inputs,
        &eff_input_cpal_indices,
        &eff_split_positions,
        &eff_entry_groups,
        &eff_outputs,
    );
    group_segments_by_input(chain, all_segments)
        .into_iter()
        .map(|(group, _segments)| group)
        .collect()
}

/// Lookup the per-route elastic target, falling back to DEFAULT_ELASTIC_TARGET
/// if the caller did not provide a value for this route index.
fn target_for_route(elastic_targets: &[usize], route_idx: usize) -> usize {
    elastic_targets
        .get(route_idx)
        .copied()
        .unwrap_or(DEFAULT_ELASTIC_TARGET)
}

pub fn build_chain_runtime_state(
    chain: &Chain,
    sample_rate: f32,
    elastic_targets: &[usize],
) -> Result<ChainRuntimeState> {
    let (eff_inputs, eff_input_cpal_indices, eff_split_positions, eff_entry_groups) =
        effective_inputs(chain);
    let eff_outputs = effective_outputs(chain);
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

/// Construct a `ChainRuntimeState` from a (possibly filtered) set of
/// segments. This is the single source of truth for runtime assembly —
/// used both by `build_chain_runtime_state` (all segments → one runtime)
/// and `build_per_input_runtimes` (one segment group → one isolated
/// per-input runtime). Each call produces its OWN `output_routes`
/// (`OutputRoutingState` + `ElasticBuffer`), `input_taps`, `stream_taps`,
/// and `processing` Mutex — nothing is shared across invocations, which is
/// what makes two per-input runtimes structurally isolated (issue #350).
///
/// `existing_blocks` (when `Some`) carries per-segment processor nodes to
/// reuse on a rebuild so a param edit does not drop audio; the outer Vec
/// is indexed by segment position within `segments`.
pub(crate) fn assemble_chain_runtime_state(
    chain: &Chain,
    segments: &[ChainSegment],
    eff_outputs: &[OutputEntry],
    sample_rate: f32,
    elastic_targets: &[usize],
    mut existing_blocks: Option<Vec<Vec<BlockRuntimeNode>>>,
) -> Result<ChainRuntimeState> {
    // Issue #592: prime the output elastic cushion only on the INITIAL
    // build (existing_blocks == None). A rebuild/edit runs warm and refills
    // naturally — re-priming on every knob turn would add a silence gap.
    let is_initial_build = existing_blocks.is_none();
    let has_convolution = crate::elastic_prime::chain_has_convolution(chain);
    let mut input_states = Vec::with_capacity(segments.len());
    for (seg_idx, segment) in segments.iter().enumerate() {
        // Determine output channels for this segment's outputs (for processing layout)
        let segment_output_channels: Vec<usize> = segment
            .output_route_indices
            .iter()
            .filter_map(|&idx| eff_outputs.get(idx))
            .flat_map(|e| e.channels.iter().copied())
            .collect();
        let existing = existing_blocks
            .as_mut()
            .and_then(|v| v.get_mut(seg_idx))
            .map(std::mem::take);
        let input_state = build_input_processing_state(
            chain,
            &segment.input,
            &segment_output_channels,
            sample_rate,
            existing,
            Some(&segment.block_indices),
            segment.output_route_indices.clone(),
            segment.split_mono_sibling_count,
        )?;
        input_states.push(input_state);
    }

    // Build input_to_segments: CPAL input_index → which (local) segments
    // to process. Indexed by the absolute cpal index so a per-input
    // runtime's slot lands where the cpal callback dispatches
    // (`process_input_f32(runtime, cpal_index, …)`); unrelated slots stay
    // empty for that runtime.
    let max_input_idx = segments
        .iter()
        .map(|s| s.cpal_input_index)
        .max()
        .unwrap_or(0);
    let mut input_to_segments: Vec<Vec<usize>> = vec![Vec::new(); max_input_idx + 1];
    for (seg_idx, segment) in segments.iter().enumerate() {
        if segment.cpal_input_index < input_to_segments.len() {
            input_to_segments[segment.cpal_input_index].push(seg_idx);
        }
    }

    let mut output_routes: Vec<Arc<OutputRoutingState>> = Vec::with_capacity(eff_outputs.len());
    for (route_idx, output) in eff_outputs.iter().enumerate() {
        let base = target_for_route(elastic_targets, route_idx);
        let target = crate::elastic_prime::elastic_capacity_target(base, has_convolution);
        let prime_frames =
            crate::elastic_prime::elastic_prime_frames(target, is_initial_build, has_convolution);
        output_routes.push(Arc::new(build_output_routing_state(
            output,
            target,
            prime_frames,
        )));
    }

    // Collect stream handles from all blocks across all input states
    let mut stream_handles_map: HashMap<BlockId, StreamHandle> = HashMap::new();
    for input_state in &input_states {
        for block in &input_state.blocks {
            if let Some(ref handle) = block.stream_handle {
                stream_handles_map.insert(block.block_id.clone(), Arc::clone(handle));
            }
        }
    }

    let input_scratches = (0..input_to_segments.len())
        .map(|_| InputCallbackScratch::default())
        .collect();

    // Issue #580: lock-free mirror of `input_states.len()` for the meter
    // polling timer to read at 30 Hz without contending with the audio
    // thread's `processing` try_lock. Captured before the Vec moves into
    // the Mutex.
    let initial_stream_count = input_states.len();
    // Issue #580 follow-up: capture the set of bypass (no live processor)
    // nodes so the GUI block-toggle fast path can decline re-enabling them
    // and fall back to a rebuild. Computed before `input_states` moves into
    // the Mutex, mirroring `initial_stream_count`.
    let initial_bypass_block_ids = collect_bypass_block_ids(&input_states);

    Ok(ChainRuntimeState {
        // Whole-chain by default; `build_per_input_runtimes` stamps the
        // per-entry identity right after assembly (issue #703).
        owned_entry: None,
        processing: Mutex::new(ChainProcessingState {
            input_states,
            input_to_segments,
            input_scratches,
        }),
        output_routes: ArcSwap::from_pointee(output_routes),
        stream_handles: Mutex::new(stream_handles_map),
        error_queue: ArrayQueue::new(ERROR_QUEUE_CAPACITY),
        created_at: std::time::Instant::now(),
        last_input_nanos: AtomicU64::new(0),
        measured_latency_nanos: AtomicU64::new(0),
        probe_state: std::sync::atomic::AtomicU8::new(PROBE_IDLE),
        draining: std::sync::atomic::AtomicBool::new(false),
        input_taps: ArcSwap::from_pointee(Vec::new()),
        stream_taps: ArcSwap::from_pointee(Vec::new()),
        output_muted: std::sync::atomic::AtomicBool::new(false),
        // Inicializa com chain.volume (issue #440). Callers que precisarem
        // de unity isolado (probe runtimes de latência) sobrescrevem com
        // `set_volume_pct(100.0)` depois.
        volume_pct_bits: std::sync::atomic::AtomicU32::new(chain.volume.to_bits()),
        stream_count: std::sync::atomic::AtomicUsize::new(initial_stream_count),
        // Issue #580 follow-up: GUI block-toggle is queued and drained
        // on the audio thread inside its own `processing` lock,
        // removing the GUI/audio Mutex contention that caused an
        // audible click on every block on/off at small buffer sizes.
        pending_block_toggles: ArrayQueue::new(64),
        bypass_block_ids: ArcSwap::from_pointee(initial_bypass_block_ids),
        di_loop: arc_swap::ArcSwapOption::empty(),
        di_loop_pos: std::sync::atomic::AtomicUsize::new(0),
        // Issue #670 — audio-thread deadline accounting, zeroed at build.
        xrun_count: AtomicU64::new(0),
        peak_load_ppm: AtomicU64::new(0),
    })
}

/// Collect the ids of every block whose live node is a
/// `RuntimeProcessor::Bypass` (built while disabled, or build-faulted).
/// These nodes have no real DSP, so re-enabling them needs a full rebuild
/// rather than the in-place fade fast path. See `ChainRuntimeState::
/// bypass_block_ids`.
fn collect_bypass_block_ids(input_states: &[InputProcessingState]) -> HashSet<BlockId> {
    let mut ids = HashSet::new();
    for input_state in input_states {
        for node in &input_state.blocks {
            if matches!(node.processor, RuntimeProcessor::Bypass) {
                ids.insert(node.block_id.clone());
            }
        }
    }
    ids
}

/// Build effective input entries from chain's InputBlock entries, plus Insert return entries.
/// Order: InputBlock entries first, then Insert return entries (matches CPAL stream order).
/// Falls back to a single mono input on channel 0 if no InputBlocks exist and no Inserts.
/// Returns (effective_entries, cpal_stream_index_per_entry).
/// cpal_stream_index maps each effective entry back to the CPAL stream
/// that provides its audio data. Entries split from the same original
/// entry share the same CPAL stream index.
fn build_input_processing_state(
    chain: &Chain,
    input: &InputEntry,
    output_channels: &[usize],
    sample_rate: f32,
    existing_blocks: Option<Vec<BlockRuntimeNode>>,
    block_indices: Option<&[usize]>,
    output_route_indices: Vec<usize>,
    split_mono_sibling_count: Option<usize>,
) -> Result<InputProcessingState> {
    // The processing bus layout is chosen by the combination of input and
    // output channel count, matching `project::chain::processing_layout`:
    //   - mono input + mono output  → Mono bus (cheaper: mono blocks skip
    //     the mono→stereo→mono round-trip)
    //   - mono input + stereo output → Stereo bus (upmix once, then stereo
    //     downstream)
    //   - stereo input               → Stereo bus
    //   - dual mono                  → Stereo bus at the buffer level,
    //     with L/R processed independently by `AudioProcessor::DualMono`
    // DualMono is flattened to Stereo at the channel-layout level because
    // the runtime tracks the independent L/R pipelines inside the processor
    // itself; the buffer between blocks only needs two slots.
    let proc_layout =
        project::chain::processing_layout(&input.channels, output_channels, input.mode);
    let input_read_layout = match input.mode {
        ChainInputMode::Mono => AudioChannelLayout::Mono,
        ChainInputMode::Stereo | ChainInputMode::DualMono => AudioChannelLayout::Stereo,
    };
    let processing_layout_channel = match proc_layout {
        project::chain::ProcessingLayout::Mono => AudioChannelLayout::Mono,
        project::chain::ProcessingLayout::Stereo | project::chain::ProcessingLayout::DualMono => {
            AudioChannelLayout::Stereo
        }
    };
    // Issue #350: split-mono segments respect the historical processing
    // layout — mono input + stereo output still upmixes (broadcast to
    // both channels) so the user hears each guitar centered. Isolation
    // between siblings is enforced at fan-out via 1/N gain reduction
    // (see `process_single_segment`), NOT by auto-panning.
    log::info!(
        "chain '{}' input entry processing layout: input_read={}, processing={:?} (channels={:?} mode={:?})",
        chain.id.0,
        layout_label(input_read_layout),
        proc_layout,
        input.channels,
        input.mode,
    );
    let had_existing = existing_blocks.is_some();
    // Issue #588: a mono source is broadcast to identical stereo channels
    // (`Stereo([s, s])`), so the content entering the first block is
    // effectively mono. A DualMono/Stereo source carries independent
    // channels and is not.
    let source_is_mono = matches!(input_read_layout, AudioChannelLayout::Mono);
    let (blocks, _output_layout) = build_runtime_block_nodes(
        chain,
        processing_layout_channel,
        source_is_mono,
        sample_rate,
        existing_blocks,
        block_indices,
    )?;

    Ok(InputProcessingState {
        input_read_layout,
        processing_layout: processing_layout_channel,
        input_channels: input.channels.clone(),
        blocks,
        frame_buffer: Vec::with_capacity(1024),
        fade_in_remaining: if had_existing { 0 } else { FADE_IN_FRAMES },
        output_route_indices,
        split_mono_sibling_count,
        outgoing: None,
    })
}

/// Channel layout an output entry produces — shared by the route builder and
/// the rebuild route-reuse check (#670).
pub(crate) fn output_entry_layout(output: &OutputEntry) -> AudioChannelLayout {
    if output.channels.len() >= 2 {
        match output.mode {
            ChainOutputMode::Stereo => AudioChannelLayout::Stereo,
            ChainOutputMode::Mono => AudioChannelLayout::Mono,
        }
    } else {
        AudioChannelLayout::Mono
    }
}

pub(crate) fn build_output_routing_state(
    output: &OutputEntry,
    elastic_target: usize,
    prime_frames: usize,
) -> OutputRoutingState {
    let output_layout = output_entry_layout(output);
    let buffer = ElasticBuffer::new(elastic_target, output_layout);
    // Issue #592: prime the cushion (silence) only when the caller asks —
    // a cold-start IR chain at a small device buffer would otherwise
    // underrun on the convolver's per-partition FFT spike.
    if prime_frames > 0 {
        buffer.prime(prime_frames);
    }
    OutputRoutingState {
        output_channels: output.channels.clone(),
        output_mixdown: ChainOutputMixdown::Average,
        buffer,
    }
}

/// In-place lock-free rebuild (param/preset edit): old processors are reused
/// and dropped. Audio is click-safe via the per-segment fade-in.
pub fn update_chain_runtime_state(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    sample_rate: f32,
    reset_output_queue: bool,
    elastic_targets: &[usize],
) -> Result<()> {
    update_chain_runtime_state_impl(
        runtime,
        chain,
        sample_rate,
        reset_output_queue,
        elastic_targets,
        false,
    )
}

/// #454-T5: same lock-free swap, but the *previous* pipeline is retained as
/// a decaying [`OutgoingTail`] on the new state so its delay/reverb tail
/// rings out in parallel (spillover) instead of being cut. The new pipeline
/// is built fresh (no processor reuse) so it fades in cleanly while the old
/// one fades out.
pub fn update_chain_runtime_state_spillover(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    sample_rate: f32,
    reset_output_queue: bool,
    elastic_targets: &[usize],
) -> Result<()> {
    update_chain_runtime_state_impl(
        runtime,
        chain,
        sample_rate,
        reset_output_queue,
        elastic_targets,
        true,
    )
}

fn update_chain_runtime_state_impl(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    sample_rate: f32,
    reset_output_queue: bool,
    elastic_targets: &[usize],
    spillover: bool,
) -> Result<()> {
    let (effective_ins, eff_input_cpal_indices, effective_split_positions, eff_entry_groups) =
        effective_inputs(chain);
    let effective_outs = effective_outputs(chain);
    let all_segments = split_chain_into_segments(
        chain,
        &effective_ins,
        &eff_input_cpal_indices,
        &effective_split_positions,
        &eff_entry_groups,
        &effective_outs,
    );
    // Issue #703: a per-entry isolated runtime is refilled with ONLY its
    // own entry's segments. Both entries of a shared device dispatch on
    // the same cpal index, so refilling every runtime with ALL segments
    // would make the one device callback process the same guitar in every
    // sibling runtime — summed at the backend mix (audible double volume).
    // A whole-chain runtime (`owned_entry == None`: probe, offline, JACK)
    // keeps every segment, exactly as before.
    let segments: Vec<ChainSegment> = match runtime.owned_entry {
        Some((group, _)) => group_segments_by_input(chain, all_segments)
            .into_iter()
            .find(|(g, _)| *g == group)
            .map(|(_, segs)| segs)
            .ok_or_else(|| {
                anyhow!(
                    "chain '{}' in-place update: entry group {} no longer exists \
                     (topology change must take the full-rebuild path)",
                    chain.id.0,
                    group
                )
            })?,
        None => all_segments,
    };

    // Step 1: Extract existing blocks from all input states (brief lock)
    let mut existing_per_input: Vec<Vec<BlockRuntimeNode>> = {
        let mut processing = lock_recover(&runtime.processing, "chain runtime");
        processing
            .input_states
            .iter_mut()
            .map(|is| std::mem::take(&mut is.blocks))
            .collect()
    };

    // Step 2: Build new input states OUTSIDE the lock (no audio interruption)
    let mut new_input_states = Vec::with_capacity(segments.len());
    for (i, segment) in segments.iter().enumerate() {
        let old_blocks = if i < existing_per_input.len() {
            std::mem::take(&mut existing_per_input[i])
        } else {
            Vec::new()
        };
        // Spillover: build the new pipeline FRESH (no processor reuse) so it
        // fades in cleanly; keep the old blocks to ring out in parallel.
        // Non-spillover: reuse old processors in place (param-edit path).
        let (existing, tail_blocks) = if spillover && !old_blocks.is_empty() {
            (None, Some(old_blocks))
        } else if old_blocks.is_empty() {
            (None, None)
        } else {
            (Some(old_blocks), None)
        };
        let segment_output_channels: Vec<usize> = segment
            .output_route_indices
            .iter()
            .filter_map(|&idx| effective_outs.get(idx))
            .flat_map(|e| e.channels.iter().copied())
            .collect();
        let input_state = match build_input_processing_state(
            chain,
            &segment.input,
            &segment_output_channels,
            sample_rate,
            existing,
            Some(&segment.block_indices),
            segment.output_route_indices.clone(),
            segment.split_mono_sibling_count,
        ) {
            Ok(state) => state,
            Err(e) => {
                // Restore previously-extracted blocks so the chain keeps playing
                log::error!(
                    "[engine] rebuild failed for chain '{}': {e} — restoring previous state",
                    chain.id.0
                );
                let mut processing = lock_recover(&runtime.processing, "chain runtime");
                for (is, old_blocks) in processing
                    .input_states
                    .iter_mut()
                    .zip(existing_per_input.into_iter())
                {
                    if is.blocks.is_empty() {
                        is.blocks = old_blocks;
                    }
                }
                return Err(e);
            }
        };
        let mut input_state = input_state;
        if let Some(blocks) = tail_blocks {
            input_state.outgoing = Some(Box::new(OutgoingTail {
                blocks,
                frames_remaining: SPILLOVER_FRAMES,
                scratch: Vec::with_capacity(2048),
            }));
        }
        new_input_states.push(input_state);
    }

    // Output routes (#670): REUSE the existing route when its endpoint shape
    // is unchanged (the param-edit / block-toggle case). A fresh empty buffer
    // here used to (a) discard the in-flight audio — the audible gap on every
    // edit — and (b) restart the standing cushion at zero, which never
    // refills in producer/consumer lockstep, leaving the chain permanently
    // fragile after the first edit (owner-reported underruns while playing,
    // reproduced by rebuild_while_playing_keeps_the_cushion). Reusing the
    // Arc keeps both the buffered audio and the cushion. A genuinely changed
    // endpoint (or an explicit queue reset) still gets a fresh route.
    let rebuild_has_convolution = crate::elastic_prime::chain_has_convolution(chain);
    let old_output_routes = runtime.output_routes.load();
    let new_output_routes: Vec<Arc<OutputRoutingState>> = effective_outs
        .iter()
        .enumerate()
        .map(|(route_idx, o)| {
            let base = target_for_route(elastic_targets, route_idx);
            let target =
                crate::elastic_prime::elastic_capacity_target(base, rebuild_has_convolution);
            if !reset_output_queue {
                if let Some(old) = old_output_routes.get(route_idx) {
                    if old.output_channels == o.channels
                        && old.buffer.layout() == output_entry_layout(o)
                        && old.buffer.target_level() == target
                    {
                        return Arc::clone(old);
                    }
                }
            }
            // Fresh route on a rebuild. A convolution chain gets the SAME
            // cushion the initial build would give it (#670): the reuse
            // check above rejects exactly when the cushion posture changed —
            // e.g. the chain GAINED its first cab/IR live — and an unprimed
            // route here left the chain permanently fragile (fill ~0, every
            // scheduling wobble on a real USB interface popped the output
            // empty: the owner's random clicks after adding/swapping a cab).
            let prime = if rebuild_has_convolution { target } else { 0 };
            let fresh = build_output_routing_state(o, target, prime);
            if let Some(old) = old_output_routes.get(route_idx) {
                fresh.buffer.seed_last_frame_from(&old.buffer);
            }
            Arc::new(fresh)
        })
        .collect();

    // Step 2.5: Refresh stream_handles — picks up new handles from rebuilt blocks
    // (e.g. block param changed → new processor → new Arc; old Arc in map would be stale)
    {
        let mut handles = lock_recover(&runtime.stream_handles, "stream_handles");
        handles.clear();
        for input_state in &new_input_states {
            for block in &input_state.blocks {
                if let Some(ref handle) = block.stream_handle {
                    handles.insert(block.block_id.clone(), Arc::clone(handle));
                }
            }
        }
    }

    // Step 3: Swap in new state (brief lock). The OLD nodes are NOT dropped
    // inside the critical section: dropping them runs the NAM C++ destructor
    // (frees the model) and the IR FFT state — multi-ms work. The audio
    // worker only try_locks `processing`; holding the lock through those
    // destructors made it emit silence for 3-6 buffers on every cab/model
    // swap (issue #670, owner's click when switching the CAB/IR — reproduced
    // on the real interface, 64-384 underruns per swap). The old Vec is moved
    // out and dropped AFTER the lock is released.
    let old_input_states;
    {
        let mut processing = lock_recover(&runtime.processing, "chain runtime");
        old_input_states = std::mem::replace(&mut processing.input_states, new_input_states);
        // Issue #580: keep the lock-free `stream_count` mirror in sync
        // with the new Vec length. Updated INSIDE the same critical
        // section that swaps the Vec so any concurrent reader sees a
        // consistent (new Vec length, new count) pair after the lock
        // releases. Relaxed ordering — the value is purely advisory for
        // the meter timer's subscription loop.
        runtime.stream_count.store(
            processing.input_states.len(),
            std::sync::atomic::Ordering::Relaxed,
        );
        // Issue #580 follow-up: refresh the lock-free bypass mirror from the
        // new nodes so re-enabling a (still) born-disabled block keeps
        // declining the fast path. Swapped inside the same critical section
        // as the Vec so a reader never sees a stale (nodes, bypass-set) pair.
        runtime
            .bypass_block_ids
            .store(Arc::new(collect_bypass_block_ids(&processing.input_states)));

        // Rebuild input_to_segments mapping from current segments
        let max_input_idx = segments
            .iter()
            .map(|s| s.cpal_input_index)
            .max()
            .unwrap_or(0);
        let mut new_mapping: Vec<Vec<usize>> = vec![Vec::new(); max_input_idx + 1];
        for (seg_idx, segment) in segments.iter().enumerate() {
            if segment.cpal_input_index < new_mapping.len() {
                new_mapping[segment.cpal_input_index].push(seg_idx);
            }
        }
        processing.input_to_segments = new_mapping;
        // Cancel any in-flight latency probe — its beep was pushed into
        // the old queue that we're about to discard, so leaving the state
        // Fired would wait forever for a detection that will never happen.
        runtime
            .probe_state
            .store(PROBE_IDLE, std::sync::atomic::Ordering::Release);
        // Resize scratches to match the new input count, preserving existing
        // allocated capacity for slots that still exist.
        let new_len = processing.input_to_segments.len();
        processing
            .input_scratches
            .resize_with(new_len, InputCallbackScratch::default);
    }
    // Lock released — NOW the old nodes (NAM models, IR FFT states) may run
    // their multi-ms destructors without starving the audio worker (#670).
    drop(old_input_states);

    // Seed each new buffer with the previous buffer's last pushed frame so a
    // brief underrun during the transition repeats the tail of the old audio
    // rather than jumping to silence. We can't migrate queued frames across
    // the swap without introducing locks, but the SPSC's underrun fallback
    // plus a matching `last_frame` makes the seam inaudible for the target
    // scenario (param tweaks that rebuild processors in place).
    if !reset_output_queue {
        let old_routes = runtime.output_routes.load();
        for (new_route, old_route) in new_output_routes.iter().zip(old_routes.iter()) {
            new_route.buffer.seed_last_frame_from(&old_route.buffer);
        }
    }
    runtime.output_routes.store(Arc::new(new_output_routes));

    // Issue #440: chain edits (incluindo o slider de volume) re-aplicam
    // o preset.volume no master output sem destruir o runtime — atomic store
    // que o audio thread vê na próxima callback.
    runtime.set_volume_pct(chain.volume);

    Ok(())
}

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
        reset_output_queue: bool,
        elastic_targets: &[usize],
    ) -> Result<Arc<ChainRuntimeState>> {
        self.upsert_chain_impl(
            chain,
            sample_rate,
            reset_output_queue,
            elastic_targets,
            false,
        )
    }

    /// #454-T5: in-place swap that lets the previous preset/scene's tail
    /// ring out in parallel (spillover). Same lock-free guarantees.
    pub fn upsert_chain_spillover(
        &mut self,
        chain: &Chain,
        sample_rate: f32,
        reset_output_queue: bool,
        elastic_targets: &[usize],
    ) -> Result<Arc<ChainRuntimeState>> {
        self.upsert_chain_impl(
            chain,
            sample_rate,
            reset_output_queue,
            elastic_targets,
            true,
        )
    }

    fn upsert_chain_impl(
        &mut self,
        chain: &Chain,
        sample_rate: f32,
        reset_output_queue: bool,
        elastic_targets: &[usize],
        spillover: bool,
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
            let mut new_groups: Vec<usize> = input_group_ids(chain);
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
                            )?;
                        } else {
                            update_chain_runtime_state(
                                runtime,
                                chain,
                                sample_rate,
                                reset_output_queue,
                                elastic_targets,
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
        for (group, state) in build_per_input_runtimes(chain, sample_rate, elastic_targets)? {
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

// Slice 4 of Phase 2: block-level builders moved to runtime_block_builders.rs.
// `runtime_graph.rs` only needs `build_runtime_block_nodes` for chain assembly.
pub(crate) use crate::runtime_block_builders::build_runtime_block_nodes;

#[cfg(test)]
#[path = "issue_592_elastic_prime_tests.rs"]
mod issue_592_elastic_prime_tests;
