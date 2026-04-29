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
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use arc_swap::ArcSwap;
use crossbeam_queue::ArrayQueue;

use block_core::{AudioChannelLayout, StreamHandle};
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlockKind, InputEntry, InsertBlock, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMixdown, ChainOutputMode};
use project::project::Project;

use crate::runtime::{
    layout_label, ChainRuntimeState, DEFAULT_ELASTIC_TARGET, FADE_IN_FRAMES, PROBE_IDLE,
};
use crate::runtime_audio_frame::ElasticBuffer;
use crate::runtime_state::{
    BlockRuntimeNode, ChainProcessingState, InputCallbackScratch, InputProcessingState,
    OutputRoutingState,
};

/// Bounded capacity for the per-chain SPSC error queue. Audio-thread
/// errors are dropped silently when the queue is full; the UI drains
/// every 200 ms, so 64 slots covers ~13 s of one-error-per-frame at
/// 48 kHz / 64-frame buffers.
pub(crate) const ERROR_QUEUE_CAPACITY: usize = 64;

pub struct RuntimeGraph {
    pub chains: HashMap<ChainId, Arc<ChainRuntimeState>>,
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
        let state = build_chain_runtime_state(chain, sample_rate, elastic_targets)?;
        chains.insert(chain.id.clone(), Arc::new(state));
    }
    Ok(RuntimeGraph { chains })
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
    let (eff_inputs, eff_input_cpal_indices, eff_split_positions) = effective_inputs(chain);
    let eff_outputs = effective_outputs(chain);
    log::info!("=== CHAIN '{}' RUNTIME BUILD ===", chain.id.0);
    log::info!("  inputs: {}", eff_inputs.len());
    for (i, inp) in eff_inputs.iter().enumerate() {
        log::info!(
            "    input[{}]: 'input #{}' dev='{}' ch={:?} cpal_stream={}",
            i,
            i,
            inp.device_id.0.split(':').last().unwrap_or("?"),
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
            out.device_id.0.split(':').last().unwrap_or("?"),
            out.channels
        );
    }
    let segments = split_chain_into_segments(
        chain,
        &eff_inputs,
        &eff_input_cpal_indices,
        &eff_split_positions,
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

    let mut input_states = Vec::with_capacity(segments.len());
    for segment in &segments {
        // Determine output channels for this segment's outputs (for processing layout)
        let segment_output_channels: Vec<usize> = segment
            .output_route_indices
            .iter()
            .filter_map(|&idx| eff_outputs.get(idx))
            .flat_map(|e| e.channels.iter().copied())
            .collect();
        let input_state = build_input_processing_state(
            chain,
            &segment.input,
            &segment_output_channels,
            sample_rate,
            None,
            Some(&segment.block_indices),
            segment.output_route_indices.clone(),
            segment.split_mono_sibling_count,
        )?;
        input_states.push(input_state);
    }

    // Build input_to_segments: CPAL input_index → which segments to process
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
        let target = target_for_route(elastic_targets, route_idx);
        output_routes.push(Arc::new(build_output_routing_state(output, target)));
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

    Ok(ChainRuntimeState {
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
    })
}

/// Build effective input entries from chain's InputBlock entries, plus Insert return entries.
/// Order: InputBlock entries first, then Insert return entries (matches CPAL stream order).
/// Falls back to a single mono input on channel 0 if no InputBlocks exist and no Inserts.
/// Returns (effective_entries, cpal_stream_index_per_entry).
/// cpal_stream_index maps each effective entry back to the CPAL stream
/// that provides its audio data. Entries split from the same original
/// entry share the same CPAL stream index.
pub(crate) fn effective_inputs(chain: &Chain) -> (Vec<InputEntry>, Vec<usize>, Vec<Option<usize>>) {
    let raw_entries: Vec<InputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib),
            _ => None,
        })
        .flat_map(|ib| ib.entries.iter().cloned())
        .collect();

    // Mono entries with multiple channels: split into one entry per channel
    // so each channel gets its own isolated processing stream. The third
    // returned vector records, per effective entry, which output-channel
    // POSITION the split-mono stream owns (`Some(0)` for the first split,
    // `Some(1)` for the second, …) so the engine can place that segment's
    // mono signal in its dedicated stereo slot at fan-out time instead of
    // broadcasting and summing on top of itself. `None` for non-split
    // entries, which keep the historical layout-aware path.
    //
    // cpal_indices maps each effective entry to the CPAL stream index.
    // Entries sharing the same device get the same CPAL stream index
    // (infra-cpal deduplicates streams by device).
    let mut entries: Vec<InputEntry> = Vec::new();
    let mut cpal_indices: Vec<usize> = Vec::new();
    let mut split_positions: Vec<Option<usize>> = Vec::new();
    let mut device_to_cpal: HashMap<String, usize> = HashMap::new();
    let mut next_cpal_idx: usize = 0;

    for entry in raw_entries.iter() {
        let device_key = entry.device_id.0.clone();
        let cpal_idx = *device_to_cpal.entry(device_key).or_insert_with(|| {
            let idx = next_cpal_idx;
            next_cpal_idx += 1;
            idx
        });

        if matches!(entry.mode, ChainInputMode::Mono) && entry.channels.len() > 1 {
            // All split siblings get the SAME sibling count (total number
            // of channels split from the original mono entry). The runtime
            // divides each segment's contribution by this count at fan-out
            // time so N loud guitars do not saturate the output limiter.
            // Mono→stereo upmix stays the historical broadcast — "mono in
            // → stereo out is broadcast to both channels" is preserved.
            let n = entry.channels.len();
            for &ch in entry.channels.iter() {
                entries.push(InputEntry {
                    device_id: entry.device_id.clone(),
                    mode: ChainInputMode::Mono,
                    channels: vec![ch],
                });
                cpal_indices.push(cpal_idx);
                split_positions.push(Some(n));
            }
        } else {
            entries.push(entry.clone());
            cpal_indices.push(cpal_idx);
            split_positions.push(None);
        }
    }

    // Append Insert return entries (as inputs for segments after each Insert)
    let insert_return_base = raw_entries.len();
    let insert_returns: Vec<InputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some(insert_return_as_input_entry(ib)),
            _ => None,
        })
        .collect();
    for (i, ret) in insert_returns.into_iter().enumerate() {
        cpal_indices.push(insert_return_base + i);
        split_positions.push(None);
        entries.push(ret);
    }

    if !entries.is_empty() {
        return (entries, cpal_indices, split_positions);
    }
    // Fallback — no InputBlocks defined
    (
        vec![InputEntry {
            device_id: domain::ids::DeviceId("".to_string()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        }],
        vec![0],
        vec![None],
    )
}

/// Build effective output entries from chain's OutputBlock entries, plus Insert send entries.
/// Order: OutputBlock entries first, then Insert send entries (matches CPAL stream order).
/// Falls back to a single mono output on channel 0 if no OutputBlocks exist and no Inserts.
pub(crate) fn effective_outputs(chain: &Chain) -> Vec<OutputEntry> {
    let mut entries: Vec<OutputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob),
            _ => None,
        })
        .flat_map(|ob| ob.entries.iter().cloned())
        .collect();

    // Append Insert send entries (as outputs for segments before each Insert)
    let insert_sends: Vec<OutputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some(insert_send_as_output_entry(ib)),
            _ => None,
        })
        .collect();
    entries.extend(insert_sends);

    if !entries.is_empty() {
        return entries;
    }
    // Fallback — no OutputBlocks defined
    vec![OutputEntry {
        device_id: domain::ids::DeviceId("".to_string()),
        mode: ChainOutputMode::Mono,
        channels: vec![0],
    }]
}

/// Convert an InsertBlock's return endpoint to an InputEntry.
pub(crate) fn insert_return_as_input_entry(insert: &InsertBlock) -> InputEntry {
    InputEntry {
        device_id: insert.return_.device_id.clone(),
        mode: insert.return_.mode,
        channels: insert.return_.channels.clone(),
    }
}

/// Convert an InsertBlock's send endpoint to an OutputEntry.
pub(crate) fn insert_send_as_output_entry(insert: &InsertBlock) -> OutputEntry {
    OutputEntry {
        device_id: insert.send.device_id.clone(),
        mode: match insert.send.mode {
            ChainInputMode::Mono => ChainOutputMode::Mono,
            _ => ChainOutputMode::Stereo,
        },
        channels: insert.send.channels.clone(),
    }
}

/// Describes a chain segment: an input source, its effect blocks, and its output targets.
#[allow(dead_code)]
pub(crate) struct ChainSegment {
    pub(crate) input: InputEntry,
    pub(crate) cpal_input_index: usize,
    pub(crate) block_indices: Vec<usize>,
    pub(crate) output_route_indices: Vec<usize>,
    /// Inherited from the originating effective input. `Some(N)` when this
    /// segment came from a split-mono entry (one InputBlock with
    /// `mode: mono` and >1 channel) and owns output channel position N.
    /// `None` for stereo / dual-mono / single-channel-mono / Insert-return
    /// segments — they keep the historical broadcast/sum behaviour.
    pub(crate) split_mono_sibling_count: Option<usize>,
}

/// Split a chain into segments at enabled Insert block boundaries.
///
/// Example: [Input, Comp, EQ, Insert, Delay, Reverb, Output]
///   Segment 1: input=InputBlock entries, blocks=[Comp, EQ], outputs=[Insert send]
///   Segment 2: input=Insert return,      blocks=[Delay, Reverb], outputs=[OutputBlock entries]
///
/// If no Insert blocks exist, a single segment covers the entire chain.
pub(crate) fn split_chain_into_segments(
    chain: &Chain,
    effective_ins: &[InputEntry],
    cpal_indices: &[usize],
    split_positions: &[Option<usize>],
    _effective_outs: &[OutputEntry],
) -> Vec<ChainSegment> {
    // Count regular InputBlock entries and OutputBlock entries
    let regular_input_count: usize = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib.entries.len()),
            _ => None,
        })
        .sum();
    let regular_output_count: usize = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob.entries.len()),
            _ => None,
        })
        .sum();

    // Find positions of enabled Insert blocks in chain.blocks
    let insert_positions: Vec<usize> = chain
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.enabled && matches!(&b.kind, AudioBlockKind::Insert(_)))
        .map(|(i, _)| i)
        .collect();

    if insert_positions.is_empty() {
        // No inserts — one segment per (input, output) pair.
        // Each output block defines a cut point: only effect blocks BEFORE that output position.

        // Find position of each enabled output block and its entry index
        let mut output_positions: Vec<(usize, usize)> = Vec::new(); // (block_pos, output_entry_idx)
        let mut out_entry_idx = 0;
        for (pos, block) in chain.blocks.iter().enumerate() {
            if block.enabled {
                if let AudioBlockKind::Output(ob) = &block.kind {
                    for _ in 0..ob.entries.len() {
                        output_positions.push((pos, out_entry_idx));
                        out_entry_idx += 1;
                    }
                }
            }
        }

        let input_count = effective_ins.len();
        let mut segments = Vec::new();

        for &(out_pos, out_entry_idx) in &output_positions {
            // Effect blocks from start up to this output position (not including I/O blocks)
            let block_indices: Vec<usize> = chain
                .blocks
                .iter()
                .enumerate()
                .filter(|(i, b)| {
                    *i < out_pos
                        && !matches!(
                            &b.kind,
                            AudioBlockKind::Input(_)
                                | AudioBlockKind::Output(_)
                                | AudioBlockKind::Insert(_)
                        )
                })
                .map(|(i, _)| i)
                .collect();

            for in_idx in 0..input_count {
                segments.push(ChainSegment {
                    input: effective_ins[in_idx].clone(),
                    cpal_input_index: cpal_indices.get(in_idx).copied().unwrap_or(in_idx),
                    block_indices: block_indices.clone(),
                    output_route_indices: vec![out_entry_idx],
                    split_mono_sibling_count: split_positions.get(in_idx).copied().unwrap_or(None),
                });
            }
        }

        return segments;
    }

    // With inserts: split into segments
    let mut segments = Vec::new();
    let mut insert_return_idx = regular_input_count; // Insert return entries start after regular inputs
    let mut insert_send_idx = regular_output_count; // Insert send entries start after regular outputs

    // Segment boundaries: [start_of_chain .. first_insert, first_insert .. second_insert, ...]
    let mut segment_start: usize = 0;
    for (insert_order, &insert_pos) in insert_positions.iter().enumerate() {
        // Effect blocks for this segment: blocks between segment_start and insert_pos
        // (excluding Input, Output, Insert routing blocks)
        let block_indices: Vec<usize> = (segment_start..insert_pos)
            .filter(|&i| {
                let b = &chain.blocks[i];
                !matches!(
                    &b.kind,
                    AudioBlockKind::Input(_)
                        | AudioBlockKind::Output(_)
                        | AudioBlockKind::Insert(_)
                )
            })
            .collect();

        // Output routes for this segment: the Insert send entry
        // Also include any OutputBlock entries that appear BEFORE this Insert
        let mut output_indices = Vec::new();
        // Regular OutputBlock entries that appear before this insert position
        let mut regular_out_idx = 0;
        for b in &chain.blocks[..insert_pos] {
            if b.enabled {
                if let AudioBlockKind::Output(ob) = &b.kind {
                    for _ in 0..ob.entries.len() {
                        output_indices.push(regular_out_idx);
                        regular_out_idx += 1;
                    }
                } else {
                    // still need to count outputs
                }
            }
        }
        // The Insert send for this segment
        output_indices.push(insert_send_idx);

        if insert_order == 0 {
            // First segment: use regular InputBlock entries
            let input_count = if regular_input_count > 0 {
                regular_input_count
            } else {
                1
            };
            for i in 0..input_count {
                segments.push(ChainSegment {
                    input: effective_ins[i].clone(),
                    cpal_input_index: i,
                    block_indices: block_indices.clone(),
                    output_route_indices: output_indices.clone(),
                    split_mono_sibling_count: split_positions.get(i).copied().unwrap_or(None),
                });
            }
        } else {
            // Subsequent segments before an insert: use previous insert's return
            let prev_return_idx = insert_return_idx - 1;
            segments.push(ChainSegment {
                input: effective_ins[prev_return_idx].clone(),
                cpal_input_index: prev_return_idx,
                block_indices,
                output_route_indices: output_indices,
                // Insert returns are not split-mono.
                split_mono_sibling_count: None,
            });
        }

        insert_return_idx += 1;
        insert_send_idx += 1;
        segment_start = insert_pos + 1;
    }

    // Final segment: after the last Insert to end of chain
    let block_indices: Vec<usize> = (segment_start..chain.blocks.len())
        .filter(|&i| {
            let b = &chain.blocks[i];
            !matches!(
                &b.kind,
                AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)
            )
        })
        .collect();

    // Output routes: regular OutputBlock entries that appear AFTER the last Insert
    let last_insert_pos = *insert_positions.last().unwrap();
    let mut output_indices = Vec::new();
    let mut regular_out_idx = 0;
    for (bi, b) in chain.blocks.iter().enumerate() {
        if b.enabled {
            if let AudioBlockKind::Output(ob) = &b.kind {
                if bi > last_insert_pos {
                    for _ in 0..ob.entries.len() {
                        output_indices.push(regular_out_idx);
                        regular_out_idx += 1;
                    }
                } else {
                    regular_out_idx += ob.entries.len();
                }
            }
        }
    }
    // If no OutputBlocks after last insert, use all regular output indices
    if output_indices.is_empty() {
        output_indices = (0..regular_output_count).collect();
    }

    // Last insert's return is the input for this segment
    let last_return_idx = insert_return_idx - 1;
    segments.push(ChainSegment {
        input: effective_ins[last_return_idx].clone(),
        cpal_input_index: last_return_idx,
        block_indices,
        output_route_indices: output_indices,
        split_mono_sibling_count: None,
    });

    segments
}

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
    let (blocks, _output_layout) = build_runtime_block_nodes(
        chain,
        processing_layout_channel,
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
    })
}

pub(crate) fn build_output_routing_state(
    output: &OutputEntry,
    elastic_target: usize,
) -> OutputRoutingState {
    let output_layout = if output.channels.len() >= 2 {
        match output.mode {
            ChainOutputMode::Stereo => AudioChannelLayout::Stereo,
            ChainOutputMode::Mono => AudioChannelLayout::Mono,
        }
    } else {
        AudioChannelLayout::Mono
    };
    OutputRoutingState {
        output_channels: output.channels.clone(),
        output_mixdown: ChainOutputMixdown::Average,
        buffer: ElasticBuffer::new(elastic_target, output_layout),
    }
}

pub fn update_chain_runtime_state(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    sample_rate: f32,
    reset_output_queue: bool,
    elastic_targets: &[usize],
) -> Result<()> {
    let (effective_ins, eff_input_cpal_indices, effective_split_positions) =
        effective_inputs(chain);
    let effective_outs = effective_outputs(chain);
    let segments = split_chain_into_segments(
        chain,
        &effective_ins,
        &eff_input_cpal_indices,
        &effective_split_positions,
        &effective_outs,
    );

    // Step 1: Extract existing blocks from all input states (brief lock)
    let mut existing_per_input: Vec<Vec<BlockRuntimeNode>> = {
        let mut processing = runtime.processing.lock().expect("chain runtime poisoned");
        processing
            .input_states
            .iter_mut()
            .map(|is| std::mem::take(&mut is.blocks))
            .collect()
    };

    // Step 2: Build new input states OUTSIDE the lock (no audio interruption)
    let mut new_input_states = Vec::with_capacity(segments.len());
    for (i, segment) in segments.iter().enumerate() {
        let existing = if i < existing_per_input.len() {
            Some(std::mem::take(&mut existing_per_input[i]))
        } else {
            None
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
                let mut processing = runtime.processing.lock().expect("chain runtime poisoned");
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
        new_input_states.push(input_state);
    }

    // Build new output routes (no per-route Mutex — ElasticBuffer is lock-free).
    let new_output_routes: Vec<Arc<OutputRoutingState>> = effective_outs
        .iter()
        .enumerate()
        .map(|(route_idx, o)| {
            let target = target_for_route(elastic_targets, route_idx);
            Arc::new(build_output_routing_state(o, target))
        })
        .collect();

    // Step 2.5: Refresh stream_handles — picks up new handles from rebuilt blocks
    // (e.g. block param changed → new processor → new Arc; old Arc in map would be stale)
    {
        let mut handles = runtime
            .stream_handles
            .lock()
            .expect("stream_handles poisoned");
        handles.clear();
        for input_state in &new_input_states {
            for block in &input_state.blocks {
                if let Some(ref handle) = block.stream_handle {
                    handles.insert(block.block_id.clone(), Arc::clone(handle));
                }
            }
        }
    }

    // Step 3: Swap in new state (brief lock)
    {
        let mut processing = runtime.processing.lock().expect("chain runtime poisoned");
        processing.input_states = new_input_states;

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

    Ok(())
}

impl RuntimeGraph {
    pub fn upsert_chain(
        &mut self,
        chain: &Chain,
        sample_rate: f32,
        reset_output_queue: bool,
        elastic_targets: &[usize],
    ) -> Result<Arc<ChainRuntimeState>> {
        if let Some(runtime) = self.chains.get(&chain.id) {
            update_chain_runtime_state(
                runtime,
                chain,
                sample_rate,
                reset_output_queue,
                elastic_targets,
            )?;
            return Ok(runtime.clone());
        }

        let state = build_chain_runtime_state(chain, sample_rate, elastic_targets)?;
        let runtime = Arc::new(state);
        self.chains.insert(chain.id.clone(), runtime.clone());
        Ok(runtime)
    }

    pub fn remove_chain(&mut self, chain_id: &ChainId) {
        self.chains.remove(chain_id);
    }

    pub fn runtime_for_chain(&self, chain_id: &ChainId) -> Option<Arc<ChainRuntimeState>> {
        self.chains.get(chain_id).cloned()
    }
}

// Slice 4 of Phase 2: block-level builders moved to runtime_block_builders.rs.
// `runtime_graph.rs` only needs `build_runtime_block_nodes` for chain assembly.
pub(crate) use crate::runtime_block_builders::build_runtime_block_nodes;
