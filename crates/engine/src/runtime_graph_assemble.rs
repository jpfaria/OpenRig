//! Chain-runtime assembly (issue #792 split from `runtime_graph.rs`).
//!
//! Setup-time only — never runs on the audio thread. This is the single
//! source of truth for turning a set of `ChainSegment`s into a fully
//! isolated `ChainRuntimeState` (its own `output_routes`, `input_taps`,
//! `stream_taps`, and `processing` Mutex — nothing shared across calls,
//! which is what makes two per-input runtimes structurally isolated,
//! issue #350).
//!
//! `runtime_graph.rs` (the graph entry points) calls
//! `assemble_chain_runtime_state`; `runtime_graph_update.rs` (the in-place
//! rebuild path) reuses the shared per-segment helpers here
//! (`build_input_processing_state`, `build_output_routing_state`,
//! `collect_bypass_block_ids`, `output_entry_layout`, `target_for_route`).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;
use crossbeam_queue::ArrayQueue;

use block_core::{AudioChannelLayout, StreamHandle};
use domain::ids::BlockId;
use project::chain::{Chain, ChainInputMode, ChainOutputMixdown, ChainOutputMode};

use crate::runtime::{
    layout_label, ChainRuntimeState, DEFAULT_ELASTIC_TARGET, FADE_IN_FRAMES, PROBE_IDLE,
};
use crate::runtime_audio_frame::ElasticBuffer;
use crate::runtime_block_builders::build_runtime_block_nodes;
use crate::runtime_endpoints::{InputEntry, OutputEntry};
use crate::runtime_graph::ERROR_QUEUE_CAPACITY;
use crate::runtime_segments::ChainSegment;
use crate::runtime_state::{
    BlockRuntimeNode, ChainProcessingState, InputCallbackScratch, InputProcessingState,
    OutputRoutingState, RuntimeProcessor,
};

/// Lookup the per-route elastic target, falling back to DEFAULT_ELASTIC_TARGET
/// if the caller did not provide a value for this route index.
pub(crate) fn target_for_route(elastic_targets: &[usize], route_idx: usize) -> usize {
    elastic_targets
        .get(route_idx)
        .copied()
        .unwrap_or(DEFAULT_ELASTIC_TARGET)
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
) -> anyhow::Result<ChainRuntimeState> {
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
        // Issue #723 — remember the real build rate so the live probe beep
        // is synthesized at the device rate, never a hardcoded 48000.
        sample_rate,
    })
}

/// Collect the ids of every block whose live node is a
/// `RuntimeProcessor::Bypass` (built while disabled, or build-faulted).
/// These nodes have no real DSP, so re-enabling them needs a full rebuild
/// rather than the in-place fade fast path. See `ChainRuntimeState::
/// bypass_block_ids`.
pub(crate) fn collect_bypass_block_ids(input_states: &[InputProcessingState]) -> HashSet<BlockId> {
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
pub(crate) fn build_input_processing_state(
    chain: &Chain,
    input: &InputEntry,
    output_channels: &[usize],
    sample_rate: f32,
    existing_blocks: Option<Vec<BlockRuntimeNode>>,
    block_indices: Option<&[usize]>,
    output_route_indices: Vec<usize>,
    split_mono_sibling_count: Option<usize>,
) -> anyhow::Result<InputProcessingState> {
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
