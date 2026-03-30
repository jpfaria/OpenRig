use anyhow::{anyhow, Result};
use domain::ids::{BlockId, ChainId};
use project::block::{
    schema_for_block_model, AudioBlockKind, CoreBlock, InputEntry, InsertBlock, NamBlock, OutputEntry, SelectBlock,
};
use project::param::ParameterSet;
use project::project::Project;
use project::chain::{
    Chain, ChainInputMode, ChainOutputMixdown, ChainOutputMode, ProcessingLayout,
};
use block_amp::build_amp_processor_for_layout;
use block_preamp::build_preamp_processor_for_layout;
use block_body::build_body_processor_for_layout;
use block_cab::build_cab_processor_for_layout;
use block_core::{
    AudioChannelLayout, ModelAudioMode, MonoProcessor, BlockProcessor, StereoProcessor,
};
use block_delay::build_delay_processor_for_layout;
use block_dyn::build_dynamics_processor_for_layout;
use block_filter::build_filter_processor_for_layout;
use block_full_rig::build_full_rig_processor_for_layout;
use block_gain::build_gain_processor_for_layout;
use block_ir::build_ir_processor_for_layout;
use block_mod::build_modulation_processor_for_layout;
use block_nam::build_nam_processor_for_layout;
use block_reverb::build_reverb_processor_for_layout;
use block_util::{build_utility_processor, TunerProcessor};
use block_wah::build_wah_processor_for_layout;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const MAX_BUFFERED_OUTPUT_FRAMES: usize = 1_024;
static NEXT_BLOCK_INSTANCE_SERIAL: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
enum AudioFrame {
    Mono(f32),
    Stereo([f32; 2]),
}

impl AudioFrame {
    fn mono_mix(self) -> f32 {
        match self {
            AudioFrame::Mono(sample) => sample,
            AudioFrame::Stereo([left, right]) => (left + right) * 0.5,
        }
    }
}

enum AudioProcessor {
    Mono(Box<dyn MonoProcessor>),
    DualMono {
        left: Box<dyn MonoProcessor>,
        right: Box<dyn MonoProcessor>,
    },
    Stereo(Box<dyn StereoProcessor>),
    StereoFromMono(Box<dyn StereoProcessor>),
}

enum ProcessorScratch {
    None,
    Mono(Vec<f32>),
    DualMono { left: Vec<f32>, right: Vec<f32> },
    Stereo(Vec<[f32; 2]>),
}

impl AudioProcessor {
    fn process_buffer(&mut self, frames: &mut [AudioFrame], scratch: &mut ProcessorScratch) {
        match (self, scratch) {
            (AudioProcessor::Mono(processor), ProcessorScratch::Mono(mono)) => {
                mono.clear();
                mono.reserve(frames.len().saturating_sub(mono.capacity()));
                for frame in frames.iter() {
                    match frame {
                        AudioFrame::Mono(sample) => mono.push(*sample),
                        AudioFrame::Stereo(_) => {
                            debug_assert!(false, "mono processor received stereo frames");
                            return;
                        }
                    }
                }
                processor.process_block(mono);
                for (frame, sample) in frames.iter_mut().zip(mono.iter().copied()) {
                    *frame = AudioFrame::Mono(sample);
                }
            }
            (
                AudioProcessor::DualMono { left, right },
                ProcessorScratch::DualMono {
                    left: left_buffer,
                    right: right_buffer,
                },
            ) => {
                left_buffer.clear();
                right_buffer.clear();
                left_buffer.reserve(frames.len().saturating_sub(left_buffer.capacity()));
                right_buffer.reserve(frames.len().saturating_sub(right_buffer.capacity()));
                for frame in frames.iter() {
                    match frame {
                        AudioFrame::Stereo([left_sample, right_sample]) => {
                            left_buffer.push(*left_sample);
                            right_buffer.push(*right_sample);
                        }
                        AudioFrame::Mono(_) => {
                            debug_assert!(false, "dual-mono processor received mono frames");
                            return;
                        }
                    }
                }
                left.process_block(left_buffer);
                right.process_block(right_buffer);
                for ((frame, left_sample), right_sample) in frames
                    .iter_mut()
                    .zip(left_buffer.iter().copied())
                    .zip(right_buffer.iter().copied())
                {
                    *frame = AudioFrame::Stereo([left_sample, right_sample]);
                }
            }
            (AudioProcessor::Stereo(processor), ProcessorScratch::Stereo(stereo)) => {
                stereo.clear();
                stereo.reserve(frames.len().saturating_sub(stereo.capacity()));
                for frame in frames.iter() {
                    match frame {
                        AudioFrame::Stereo(stereo_frame) => stereo.push(*stereo_frame),
                        AudioFrame::Mono(_) => {
                            debug_assert!(false, "stereo processor received mono frames");
                            return;
                        }
                    }
                }
                processor.process_block(stereo);
                for (frame, stereo_frame) in frames.iter_mut().zip(stereo.iter().copied()) {
                    *frame = AudioFrame::Stereo(stereo_frame);
                }
            }
            (AudioProcessor::StereoFromMono(processor), ProcessorScratch::Stereo(stereo)) => {
                stereo.clear();
                stereo.reserve(frames.len().saturating_sub(stereo.capacity()));
                for frame in frames.iter() {
                    match frame {
                        AudioFrame::Mono(sample) => stereo.push([*sample, *sample]),
                        AudioFrame::Stereo(_) => {
                            debug_assert!(false, "mono-to-stereo processor received stereo frames");
                            return;
                        }
                    }
                }
                processor.process_block(stereo);
                for (frame, stereo_frame) in frames.iter_mut().zip(stereo.iter().copied()) {
                    *frame = AudioFrame::Stereo(stereo_frame);
                }
            }
            _ => {
                debug_assert!(false, "processor scratch layout mismatch");
            }
        }
    }
}

pub struct ChainRuntimeState {
    processing: Mutex<ChainProcessingState>,
    output: Mutex<ChainOutputState>,
    /// Tuner samples written by audio thread, read+cleared by UI thread.
    tuner_shared_buffer: Mutex<Vec<f32>>,
    /// Tuner state owned by UI thread only (never touched by audio).
    pub tuner_reading: Mutex<block_util::TunerReading>,
}

impl ChainRuntimeState {
    /// Called from audio thread: append tuner samples (fast, non-blocking).
    pub fn push_tuner_samples(&self, samples: &[f32]) {
        if let Ok(mut buf) = self.tuner_shared_buffer.try_lock() {
            buf.extend_from_slice(samples);
            // Cap at 8192 to prevent unbounded growth
            if buf.len() > 8192 {
                let start = buf.len() - 4096;
                buf.drain(..start);
            }
        }
    }

    /// Called from UI thread: run detection on accumulated samples.
    pub fn poll_tuner(&self) -> Option<block_util::TunerReading> {
        // Grab samples quickly
        let samples = {
            let mut buf = self.tuner_shared_buffer.try_lock().ok()?;
            if buf.len() < 2048 {
                return self.tuner_reading.try_lock().ok().and_then(|r| {
                    if r.frequency.is_some() { Some(r.clone()) } else { None }
                });
            }
            let s = buf.clone();
            buf.clear();
            s
        };

        // Run detection outside any lock (takes ~1ms, fine for UI thread)
        let reading = detect_pitch(&samples);

        // Store for next poll if no detection
        if let Ok(mut tr) = self.tuner_reading.try_lock() {
            if reading.frequency.is_some() {
                *tr = reading.clone();
            }
        }

        if reading.frequency.is_some() { Some(reading) } else {
            self.tuner_reading.try_lock().ok().and_then(|r| {
                if r.frequency.is_some() { Some(r.clone()) } else { None }
            })
        }
    }
}

/// Number of frames to fade in after a chain rebuild to avoid clicks/pops.
const FADE_IN_FRAMES: usize = 128;

struct InputProcessingState {
    input_read_layout: AudioChannelLayout,
    processing_layout: AudioChannelLayout,
    input_channels: Vec<usize>,
    blocks: Vec<BlockRuntimeNode>,
    frame_buffer: Vec<AudioFrame>,
    /// Remaining frames of fade-in after a rebuild (0 = no fade active).
    fade_in_remaining: usize,
    /// Which output route indices this input/segment should push frames to.
    /// Empty means push to ALL output routes (legacy behaviour).
    output_route_indices: Vec<usize>,
}

struct ChainProcessingState {
    input_states: Vec<InputProcessingState>,
    tuner_samples: Vec<f32>,
    #[allow(dead_code)]
    mixed_buffer: Vec<AudioFrame>,
}

struct OutputRoutingState {
    output_layout: AudioChannelLayout,
    output_channels: Vec<usize>,
    output_mixdown: ChainOutputMixdown,
    queue: VecDeque<AudioFrame>,
}

struct ChainOutputState {
    output_routes: Vec<OutputRoutingState>,
}

enum RuntimeProcessor {
    Audio(AudioProcessor),
    #[allow(dead_code)]
    Tuner(Box<dyn TunerProcessor>),
    Select(SelectRuntimeState),
    Bypass,
}

struct BlockRuntimeNode {
    #[cfg_attr(not(test), allow(dead_code))]
    instance_serial: u64,
    block_id: BlockId,
    block_snapshot: project::block::AudioBlock,
    input_layout: AudioChannelLayout,
    output_layout: AudioChannelLayout,
    scratch: ProcessorScratch,
    processor: RuntimeProcessor,
}

struct SelectRuntimeState {
    selected_block_id: BlockId,
    options: Vec<BlockRuntimeNode>,
}

struct ProcessorBuildOutcome {
    processor: AudioProcessor,
    output_layout: AudioChannelLayout,
}

impl SelectRuntimeState {
    fn selected_node(&self) -> Option<&BlockRuntimeNode> {
        self.options
            .iter()
            .find(|option| option.block_id == self.selected_block_id)
    }

    fn selected_node_mut(&mut self) -> Option<&mut BlockRuntimeNode> {
        self.options
            .iter_mut()
            .find(|option| option.block_id == self.selected_block_id)
    }
}

pub struct RuntimeGraph {
    pub chains: HashMap<ChainId, Arc<ChainRuntimeState>>,
}

pub fn build_runtime_graph(
    project: &Project,
    chain_sample_rates: &HashMap<ChainId, f32>,
) -> Result<RuntimeGraph> {
    let mut chains = HashMap::new();
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        let sample_rate = *chain_sample_rates
            .get(&chain.id)
            .ok_or_else(|| anyhow!("chain '{}' has no resolved runtime sample rate", chain.id.0))?;
        let state = build_chain_runtime_state(chain, sample_rate)?;
        chains.insert(chain.id.clone(), Arc::new(state));
    }
    Ok(RuntimeGraph { chains })
}

pub fn build_chain_runtime_state(chain: &Chain, sample_rate: f32) -> Result<ChainRuntimeState> {
    let eff_inputs = effective_inputs(chain);
    let eff_outputs = effective_outputs(chain);
    log::info!("[build_chain_runtime] chain='{}' eff_inputs={} eff_outputs={}", chain.id.0, eff_inputs.len(), eff_outputs.len());
    for (i, inp) in eff_inputs.iter().enumerate() {
        log::info!("[build_chain_runtime]   input[{}]: name='{}' device='{}' channels={:?}", i, inp.name, inp.device_id.0, inp.channels);
    }
    for (i, out) in eff_outputs.iter().enumerate() {
        log::info!("[build_chain_runtime]   output[{}]: name='{}' device='{}' channels={:?}", i, out.name, out.device_id.0, out.channels);
    }
    let segments = split_chain_into_segments(chain, &eff_inputs, &eff_outputs);
    log::info!("[build_chain_runtime] segments={}", segments.len());
    for (i, seg) in segments.iter().enumerate() {
        log::info!("[build_chain_runtime]   segment[{}]: input='{}' blocks={:?} output_routes={:?}", i, seg.input.name, seg.block_indices, seg.output_route_indices);
    }

    let mut input_states = Vec::with_capacity(segments.len());
    for segment in &segments {
        // Determine output channels for this segment's outputs (for processing layout)
        let segment_output_channels: Vec<usize> = segment.output_route_indices.iter()
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
        )?;
        input_states.push(input_state);
    }

    let mut output_routes = Vec::with_capacity(eff_outputs.len());
    for output in &eff_outputs {
        output_routes.push(build_output_routing_state(output));
    }

    Ok(ChainRuntimeState {
        processing: Mutex::new(ChainProcessingState {
            input_states,
            tuner_samples: Vec::new(),
            mixed_buffer: Vec::with_capacity(1024),
        }),
        output: Mutex::new(ChainOutputState { output_routes }),
        tuner_shared_buffer: Mutex::new(Vec::with_capacity(8192)),
        tuner_reading: Mutex::new(block_util::TunerReading::default()),
    })
}

/// Build effective input entries from chain's InputBlock entries, plus Insert return entries.
/// Order: InputBlock entries first, then Insert return entries (matches CPAL stream order).
/// Falls back to a single mono input on channel 0 if no InputBlocks exist and no Inserts.
fn effective_inputs(chain: &Chain) -> Vec<InputEntry> {
    let mut entries: Vec<InputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib),
            _ => None,
        })
        .flat_map(|ib| ib.entries.iter().cloned())
        .collect();

    // Append Insert return entries (as inputs for segments after each Insert)
    let insert_returns: Vec<InputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some(insert_return_as_input_entry(ib)),
            _ => None,
        })
        .collect();
    entries.extend(insert_returns);

    if !entries.is_empty() {
        return entries;
    }
    // Fallback — no InputBlocks defined
    vec![InputEntry {
        name: "Input".to_string(),
        device_id: domain::ids::DeviceId("".to_string()),
        mode: ChainInputMode::Mono,
        channels: vec![0],
    }]
}

/// Build effective output entries from chain's OutputBlock entries, plus Insert send entries.
/// Order: OutputBlock entries first, then Insert send entries (matches CPAL stream order).
/// Falls back to a single mono output on channel 0 if no OutputBlocks exist and no Inserts.
fn effective_outputs(chain: &Chain) -> Vec<OutputEntry> {
    let mut entries: Vec<OutputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob),
            _ => None,
        })
        .flat_map(|ob| ob.entries.iter().cloned())
        .collect();

    // Append Insert send entries (as outputs for segments before each Insert)
    let insert_sends: Vec<OutputEntry> = chain.blocks.iter()
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
        name: "Output".to_string(),
        device_id: domain::ids::DeviceId("".to_string()),
        mode: ChainOutputMode::Mono,
        channels: vec![0],
    }]
}

/// Convert an InsertBlock's return endpoint to an InputEntry.
fn insert_return_as_input_entry(insert: &InsertBlock) -> InputEntry {
    InputEntry {
        name: "Insert Return".to_string(),
        device_id: insert.return_.device_id.clone(),
        mode: insert.return_.mode,
        channels: insert.return_.channels.clone(),
    }
}

/// Convert an InsertBlock's send endpoint to an OutputEntry.
fn insert_send_as_output_entry(insert: &InsertBlock) -> OutputEntry {
    OutputEntry {
        name: "Insert Send".to_string(),
        device_id: insert.send.device_id.clone(),
        mode: match insert.send.mode {
            ChainInputMode::Mono => ChainOutputMode::Mono,
            _ => ChainOutputMode::Stereo,
        },
        channels: insert.send.channels.clone(),
    }
}

/// Describes a chain segment: an input source, its effect blocks, and its output targets.
struct ChainSegment {
    input: InputEntry,
    /// Indices of the AudioBlocks in chain.blocks that belong to this segment's effect chain.
    block_indices: Vec<usize>,
    /// Indices into the effective_outputs list that this segment pushes to.
    output_route_indices: Vec<usize>,
}

/// Split a chain into segments at enabled Insert block boundaries.
///
/// Example: [Input, Comp, EQ, Insert, Delay, Reverb, Output]
///   Segment 1: input=InputBlock entries, blocks=[Comp, EQ], outputs=[Insert send]
///   Segment 2: input=Insert return,      blocks=[Delay, Reverb], outputs=[OutputBlock entries]
///
/// If no Insert blocks exist, a single segment covers the entire chain.
fn split_chain_into_segments(chain: &Chain, effective_ins: &[InputEntry], effective_outs: &[OutputEntry]) -> Vec<ChainSegment> {
    // Count regular InputBlock entries and OutputBlock entries
    let regular_input_count: usize = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib.entries.len()),
            _ => None,
        })
        .sum();
    let regular_output_count: usize = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob.entries.len()),
            _ => None,
        })
        .sum();

    // Find positions of enabled Insert blocks in chain.blocks
    let insert_positions: Vec<usize> = chain.blocks.iter()
        .enumerate()
        .filter(|(_, b)| b.enabled && matches!(&b.kind, AudioBlockKind::Insert(_)))
        .map(|(i, _)| i)
        .collect();

    if insert_positions.is_empty() {
        // No inserts — single segment, all inputs push to all outputs
        let block_indices: Vec<usize> = chain.blocks.iter()
            .enumerate()
            .filter(|(_, b)| !matches!(&b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)))
            .map(|(i, _)| i)
            .collect();
        let all_output_indices: Vec<usize> = (0..effective_outs.len()).collect();
        // One segment per regular input entry
        return effective_ins.iter().enumerate().map(|(_i, input)| {
            ChainSegment {
                input: input.clone(),
                block_indices: block_indices.clone(),
                output_route_indices: all_output_indices.clone(),
            }
        }).filter(|_| true) // keep type inference happy
        .take(if regular_input_count > 0 { regular_input_count } else { effective_ins.len() })
        .collect();
    }

    // With inserts: split into segments
    let mut segments = Vec::new();
    let mut insert_return_idx = regular_input_count; // Insert return entries start after regular inputs
    let mut insert_send_idx = regular_output_count;  // Insert send entries start after regular outputs

    // Segment boundaries: [start_of_chain .. first_insert, first_insert .. second_insert, ...]
    let mut segment_start: usize = 0;
    for (insert_order, &insert_pos) in insert_positions.iter().enumerate() {
        // Effect blocks for this segment: blocks between segment_start and insert_pos
        // (excluding Input, Output, Insert routing blocks)
        let block_indices: Vec<usize> = (segment_start..insert_pos)
            .filter(|&i| {
                let b = &chain.blocks[i];
                !matches!(&b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_))
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
            let input_count = if regular_input_count > 0 { regular_input_count } else { 1 };
            for i in 0..input_count {
                segments.push(ChainSegment {
                    input: effective_ins[i].clone(),
                    block_indices: block_indices.clone(),
                    output_route_indices: output_indices.clone(),
                });
            }
        } else {
            // Subsequent segments before an insert: use previous insert's return
            let prev_return_idx = insert_return_idx - 1;
            segments.push(ChainSegment {
                input: effective_ins[prev_return_idx].clone(),
                block_indices,
                output_route_indices: output_indices,
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
            !matches!(&b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_))
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
        block_indices,
        output_route_indices: output_indices,
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
) -> Result<InputProcessingState> {
    // Use both input and output channel info to determine processing layout
    // (matches legacy behavior: mono input + stereo output = stereo processing)
    let proc_layout = project::chain::processing_layout(
        &input.channels,
        output_channels,
        input.mode,
    );
    let processing_layout_channel = match proc_layout {
        ProcessingLayout::Mono | ProcessingLayout::DualMono => AudioChannelLayout::Mono,
        ProcessingLayout::Stereo => AudioChannelLayout::Stereo,
    };
    let input_read_layout = match input.mode {
        ChainInputMode::Mono => AudioChannelLayout::Mono,
        ChainInputMode::Stereo | ChainInputMode::DualMono => AudioChannelLayout::Stereo,
    };
    log::info!(
        "chain '{}' input entry processing layout: input_read={}, processing={:?} (channels={:?} mode={:?})",
        chain.id.0,
        layout_label(input_read_layout),
        proc_layout,
        input.channels,
        input.mode,
    );
    let (blocks, _output_layout) =
        build_runtime_block_nodes(chain, processing_layout_channel, sample_rate, existing_blocks, block_indices)?;

    Ok(InputProcessingState {
        input_read_layout,
        processing_layout: processing_layout_channel,
        input_channels: input.channels.clone(),
        blocks,
        frame_buffer: Vec::with_capacity(1024),
        fade_in_remaining: FADE_IN_FRAMES,
        output_route_indices,
    })
}

fn build_output_routing_state(output: &OutputEntry) -> OutputRoutingState {
    let output_layout = if output.channels.len() >= 2 {
        match output.mode {
            ChainOutputMode::Stereo => AudioChannelLayout::Stereo,
            ChainOutputMode::Mono => AudioChannelLayout::Mono,
        }
    } else {
        AudioChannelLayout::Mono
    };
    OutputRoutingState {
        output_layout,
        output_channels: output.channels.clone(),
        output_mixdown: ChainOutputMixdown::Average,
        queue: VecDeque::with_capacity(MAX_BUFFERED_OUTPUT_FRAMES),
    }
}

pub fn update_chain_runtime_state(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    sample_rate: f32,
    reset_output_queue: bool,
) -> Result<()> {
    let effective_ins = effective_inputs(chain);
    let effective_outs = effective_outputs(chain);
    log::info!("[update_runtime] chain='{}' inputs={} outputs={}", chain.id.0, effective_ins.len(), effective_outs.len());
    for (i, inp) in effective_ins.iter().enumerate() {
        log::info!("[update_runtime]   in[{}]: '{}' dev='{}' ch={:?}", i, inp.name, inp.device_id.0, inp.channels);
    }
    for (i, out) in effective_outs.iter().enumerate() {
        log::info!("[update_runtime]   out[{}]: '{}' dev='{}' ch={:?}", i, out.name, out.device_id.0, out.channels);
    }
    let segments = split_chain_into_segments(chain, &effective_ins, &effective_outs);
    log::info!("[update_runtime] segments={}", segments.len());
    for (i, seg) in segments.iter().enumerate() {
        log::info!("[update_runtime]   seg[{}]: in='{}' blocks={:?} out_routes={:?}", i, seg.input.name, seg.block_indices, seg.output_route_indices);
    }

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
        let segment_output_channels: Vec<usize> = segment.output_route_indices.iter()
            .filter_map(|&idx| effective_outs.get(idx))
            .flat_map(|e| e.channels.iter().copied())
            .collect();
        let input_state = build_input_processing_state(
            chain,
            &segment.input,
            &segment_output_channels,
            sample_rate,
            existing,
            Some(&segment.block_indices),
            segment.output_route_indices.clone(),
        )?;
        new_input_states.push(input_state);
    }

    // Build new output routes
    let new_output_routes: Vec<OutputRoutingState> = effective_outs
        .iter()
        .map(|o| build_output_routing_state(o))
        .collect();

    // Step 3: Swap in new state (brief lock)
    {
        let mut processing = runtime.processing.lock().expect("chain runtime poisoned");
        processing.input_states = new_input_states;
    }

    {
        let mut output = runtime.output.lock().expect("chain runtime poisoned");
        // Preserve existing queues where possible
        let old_routes = std::mem::replace(&mut output.output_routes, new_output_routes);
        for (new_route, old_route) in output.output_routes.iter_mut().zip(old_routes.into_iter()) {
            if !reset_output_queue {
                new_route.queue = old_route.queue;
                trim_output_queue(&mut new_route.queue);
            }
        }
    }

    Ok(())
}

impl RuntimeGraph {
    pub fn upsert_chain(
        &mut self,
        chain: &Chain,
        sample_rate: f32,
        reset_output_queue: bool,
    ) -> Result<Arc<ChainRuntimeState>> {
        if let Some(runtime) = self.chains.get(&chain.id) {
            update_chain_runtime_state(runtime, chain, sample_rate, reset_output_queue)?;
            return Ok(runtime.clone());
        }

        let state = build_chain_runtime_state(chain, sample_rate)?;
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

fn build_runtime_block_nodes(
    chain: &Chain,
    input_layout: AudioChannelLayout,
    sample_rate: f32,
    existing: Option<Vec<BlockRuntimeNode>>,
    block_indices: Option<&[usize]>,
) -> Result<(Vec<BlockRuntimeNode>, AudioChannelLayout)> {
    let mut blocks = Vec::new();
    let mut current_layout = input_layout;
    let mut reusable_nodes = existing
        .unwrap_or_default()
        .into_iter()
        .map(|node| (node.block_id.clone(), node))
        .collect::<HashMap<_, _>>();

    // If block_indices is provided, iterate only those blocks; otherwise iterate all
    let block_iter: Vec<&project::block::AudioBlock> = match block_indices {
        Some(indices) => indices.iter()
            .filter_map(|&i| chain.blocks.get(i))
            .collect(),
        None => chain.blocks.iter().collect(),
    };

    for block in block_iter {
        // Disabled blocks: try to reuse existing node (keeps processor alive
        // for instant re-enable), otherwise create a bypass node.
        if !block.enabled {
            if let Some(mut node) = reusable_nodes.remove(&block.id) {
                node.block_snapshot = block.clone();
                // Keep the processor alive but don't change layout
                blocks.push(node);
            } else {
                blocks.push(bypass_runtime_node(block, current_layout));
            }
            continue;
        }
        // Input/Output/Insert blocks are routing metadata; skip them in the processing chain
        if matches!(&block.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)) {
            continue;
        }
        if let AudioBlockKind::Select(select) = &block.kind {
            let existing_select_node = reusable_nodes
                .remove(&block.id)
                .filter(|node| node.input_layout == current_layout);
            let node = build_select_runtime_node(
                chain,
                block,
                select,
                current_layout,
                sample_rate,
                existing_select_node,
            )?;
            current_layout = node.output_layout;
            blocks.push(node);
            continue;
        }
        if let Some(node) = try_reuse_block_node(&mut reusable_nodes, block, current_layout) {
            log::info!("[engine] reuse block {:?} (id={})", block.model_ref().map(|m| m.model), block.id.0);
            current_layout = node.output_layout;
            blocks.push(node);
            continue;
        }

        log::info!("[engine] rebuild block {:?} (id={}) with params:", block.model_ref().map(|m| m.model), block.id.0);
        if let Some(model) = block.model_ref() {
            for (path, value) in model.params.values.iter() {
                log::info!("[engine]   {} = {:?}", path, value);
            }
        }
        let node = build_block_runtime_node(chain, block, current_layout, sample_rate)?;
        current_layout = node.output_layout;
        blocks.push(node);
    }

    Ok((blocks, current_layout))
}

fn try_reuse_block_node(
    reusable_nodes: &mut HashMap<BlockId, BlockRuntimeNode>,
    block: &project::block::AudioBlock,
    current_layout: AudioChannelLayout,
) -> Option<BlockRuntimeNode> {
    let mut node = reusable_nodes.remove(&block.id)?;
    if node.input_layout != current_layout {
        return None;
    }
    // Exact match — reuse as-is
    if node.block_snapshot == *block {
        return Some(node);
    }
    // Only enabled changed — reuse processor, update snapshot
    let mut snapshot_without_enabled = node.block_snapshot.clone();
    snapshot_without_enabled.enabled = block.enabled;
    if snapshot_without_enabled == *block {
        node.block_snapshot = block.clone();
        return Some(node);
    }
    None
}

fn build_block_runtime_node(
    chain: &Chain,
    block: &project::block::AudioBlock,
    input_layout: AudioChannelLayout,
    sample_rate: f32,
) -> Result<BlockRuntimeNode> {
    Ok(match &block.kind {
        _ if !block.enabled => bypass_runtime_node(block, input_layout),
        AudioBlockKind::Nam(stage) => audio_block_runtime_node(
            block,
            input_layout,
            build_nam_audio_processor(chain, stage, input_layout, sample_rate)?,
        ),
        AudioBlockKind::Core(core) => build_core_block_runtime_node(chain, block, core, input_layout, sample_rate)?,
        AudioBlockKind::Select(select) => {
            build_select_runtime_node(chain, block, select, input_layout, sample_rate, None)?
        }
        // Input/Output/Insert blocks are routing-only; they don't process audio in the block chain
        AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => bypass_runtime_node(block, input_layout),
    })
}

fn build_core_block_runtime_node(
    chain: &Chain,
    block: &project::block::AudioBlock,
    core: &CoreBlock,
    input_layout: AudioChannelLayout,
    sample_rate: f32,
) -> Result<BlockRuntimeNode> {
    let effect_type = core.effect_type.as_str();
    let model = &core.model;
    let params = &core.params;

    use block_core::*;
    match effect_type {
        EFFECT_TYPE_PREAMP => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_PREAMP, model, input_layout, |layout| {
                build_preamp_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_AMP => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_AMP, model, input_layout, |layout| {
                build_amp_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_FULL_RIG => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_FULL_RIG, model, input_layout, |layout| {
                build_full_rig_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_CAB => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_CAB, model, input_layout, |layout| {
                build_cab_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_BODY => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_BODY, model, input_layout, |layout| {
                build_body_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_IR => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_IR, model, input_layout, |layout| {
                build_ir_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_GAIN => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_GAIN, model, input_layout, |layout| {
                build_gain_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_DELAY => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_DELAY, model, input_layout, |layout| {
                build_delay_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_REVERB => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_REVERB, model, input_layout, |layout| {
                build_reverb_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_UTILITY => Ok(BlockRuntimeNode {
            instance_serial: next_block_instance_serial(),
            block_id: block.id.clone(),
            block_snapshot: block.clone(),
            input_layout,
            output_layout: input_layout,
            scratch: ProcessorScratch::Mono(Vec::new()),
            processor: RuntimeProcessor::Tuner(build_utility_processor(
                model,
                params,
                sample_rate.round() as usize,
            )?),
        }),
        EFFECT_TYPE_DYNAMICS => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_DYNAMICS, model, input_layout, |layout| {
                build_dynamics_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_FILTER => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_FILTER, model, input_layout, |layout| {
                build_filter_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_WAH => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_WAH, model, input_layout, |layout| {
                build_wah_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_MODULATION => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_MODULATION, model, input_layout, |layout| {
                build_modulation_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        EFFECT_TYPE_PITCH => Ok(bypass_runtime_node(block, input_layout)),
        other => Err(anyhow!("unsupported core block effect_type '{}'", other)),
    }
}

fn build_select_runtime_node(
    chain: &Chain,
    block: &project::block::AudioBlock,
    select: &SelectBlock,
    input_layout: AudioChannelLayout,
    sample_rate: f32,
    existing: Option<BlockRuntimeNode>,
) -> Result<BlockRuntimeNode> {
    let (instance_serial, mut reusable_option_nodes) = match existing {
        Some(node) => {
            let instance_serial = node.instance_serial;
            let options = match node.processor {
                RuntimeProcessor::Select(select_runtime) => select_runtime
                    .options
                    .into_iter()
                    .map(|option| (option.block_id.clone(), option))
                    .collect::<HashMap<_, _>>(),
                _ => HashMap::new(),
            };
            (instance_serial, options)
        }
        None => (next_block_instance_serial(), HashMap::new()),
    };

    let mut option_nodes = Vec::with_capacity(select.options.len());
    let mut resolved_output_layout = None;
    for option in &select.options {
        let option_node = if let Some(node) =
            try_reuse_block_node(&mut reusable_option_nodes, option, input_layout)
        {
            node
        } else {
            build_block_runtime_node(chain, option, input_layout, sample_rate)?
        };
        if let Some(existing_layout) = resolved_output_layout {
            if existing_layout != option_node.output_layout {
                return Err(anyhow!(
                    "chain '{}' select block '{}' mixes incompatible output layouts across options",
                    chain.id.0,
                    block.id.0
                ));
            }
        } else {
            resolved_output_layout = Some(option_node.output_layout);
        }
        option_nodes.push(option_node);
    }

    let output_layout = option_nodes
        .iter()
        .find(|option| option.block_id == select.selected_block_id)
        .map(|option| option.output_layout)
        .ok_or_else(|| anyhow!("chain '{}' select block references unknown option", chain.id.0))?;

    Ok(BlockRuntimeNode {
        instance_serial,
        block_id: block.id.clone(),
        block_snapshot: block.clone(),
        input_layout,
        output_layout,
        scratch: ProcessorScratch::None,
        processor: RuntimeProcessor::Select(SelectRuntimeState {
            selected_block_id: select.selected_block_id.clone(),
            options: option_nodes,
        }),
    })
}

fn bypass_runtime_node(
    block: &project::block::AudioBlock,
    input_layout: AudioChannelLayout,
) -> BlockRuntimeNode {
    BlockRuntimeNode {
        instance_serial: next_block_instance_serial(),
        block_id: block.id.clone(),
        block_snapshot: block.clone(),
        input_layout,
        output_layout: input_layout,
        scratch: ProcessorScratch::None,
        processor: RuntimeProcessor::Bypass,
    }
}

fn audio_block_runtime_node(
    block: &project::block::AudioBlock,
    input_layout: AudioChannelLayout,
    outcome: ProcessorBuildOutcome,
) -> BlockRuntimeNode {
    let scratch = processor_scratch(&outcome.processor);
    BlockRuntimeNode {
        instance_serial: next_block_instance_serial(),
        block_id: block.id.clone(),
        block_snapshot: block.clone(),
        input_layout,
        output_layout: outcome.output_layout,
        scratch,
        processor: RuntimeProcessor::Audio(outcome.processor),
    }
}

fn processor_scratch(processor: &AudioProcessor) -> ProcessorScratch {
    match processor {
        AudioProcessor::Mono(_) => ProcessorScratch::Mono(Vec::new()),
        AudioProcessor::DualMono { .. } => ProcessorScratch::DualMono {
            left: Vec::new(),
            right: Vec::new(),
        },
        AudioProcessor::Stereo(_) | AudioProcessor::StereoFromMono(_) => {
            ProcessorScratch::Stereo(Vec::new())
        }
    }
}

fn build_audio_processor_for_model<F>(
    chain: &Chain,
    effect_type: &str,
    model: &str,
    input_layout: AudioChannelLayout,
    mut builder: F,
) -> Result<ProcessorBuildOutcome>
where
    F: FnMut(AudioChannelLayout) -> Result<BlockProcessor>,
{
    let schema = schema_for_block_model(effect_type, model).map_err(|error| {
        anyhow!(
            "chain '{}' {} model '{}': {}",
            chain.id.0,
            effect_type,
            model,
            error
        )
    })?;

    let output_layout = schema
        .audio_mode
        .output_layout(input_layout)
        .ok_or_else(|| {
            anyhow!(
                "chain '{}' {} model '{}' with audio mode '{}' does not accept {} input",
                chain.id.0,
                effect_type,
                model,
                schema.audio_mode.as_str(),
                layout_label(input_layout)
            )
        })?;

    let processor = match (schema.audio_mode, input_layout) {
        (ModelAudioMode::MonoOnly, AudioChannelLayout::Mono) => {
            AudioProcessor::Mono(expect_mono_processor(
                builder(AudioChannelLayout::Mono)?,
                chain,
                effect_type,
                model,
            )?)
        }
        (ModelAudioMode::DualMono, AudioChannelLayout::Mono) => {
            AudioProcessor::Mono(expect_mono_processor(
                builder(AudioChannelLayout::Mono)?,
                chain,
                effect_type,
                model,
            )?)
        }
        (ModelAudioMode::DualMono, AudioChannelLayout::Stereo) => AudioProcessor::DualMono {
            left: expect_mono_processor(
                builder(AudioChannelLayout::Mono)?,
                chain,
                effect_type,
                model,
            )?,
            right: expect_mono_processor(
                builder(AudioChannelLayout::Mono)?,
                chain,
                effect_type,
                model,
            )?,
        },
        (ModelAudioMode::TrueStereo, AudioChannelLayout::Stereo) => {
            AudioProcessor::Stereo(expect_stereo_processor(
                builder(AudioChannelLayout::Stereo)?,
                chain,
                effect_type,
                model,
            )?)
        }
        (ModelAudioMode::MonoToStereo, AudioChannelLayout::Mono) => {
            AudioProcessor::StereoFromMono(expect_stereo_processor(
                builder(AudioChannelLayout::Stereo)?,
                chain,
                effect_type,
                model,
            )?)
        }
        (ModelAudioMode::MonoToStereo, AudioChannelLayout::Stereo) => {
            AudioProcessor::Stereo(expect_stereo_processor(
                builder(AudioChannelLayout::Stereo)?,
                chain,
                effect_type,
                model,
            )?)
        }
        _ => {
            return Err(anyhow!(
                "chain '{}' {} model '{}' with audio mode '{}' cannot run on {} input",
                chain.id.0,
                effect_type,
                model,
                schema.audio_mode.as_str(),
                layout_label(input_layout)
            ));
        }
    };

    Ok(ProcessorBuildOutcome {
        processor,
        output_layout,
    })
}

fn build_nam_audio_processor(
    chain: &Chain,
    stage: &NamBlock,
    input_layout: AudioChannelLayout,
    sample_rate: f32,
) -> Result<ProcessorBuildOutcome> {
    let _ = (
        optional_string(&stage.params, "ir_path"),
        required_string(&stage.params, "model_path")?,
    );
    build_audio_processor_for_model(chain, block_core::EFFECT_TYPE_NAM, &stage.model, input_layout, |layout| {
        build_nam_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
    })
}

fn expect_mono_processor(
    processor: BlockProcessor,
    chain: &Chain,
    effect_type: &str,
    model: &str,
) -> Result<Box<dyn MonoProcessor>> {
    match processor {
        BlockProcessor::Mono(processor) => Ok(processor),
        BlockProcessor::Stereo(_) => Err(anyhow!(
            "chain '{}' {} model '{}' returned stereo processing where mono was required",
            chain.id.0,
            effect_type,
            model
        )),
    }
}

fn expect_stereo_processor(
    processor: BlockProcessor,
    chain: &Chain,
    effect_type: &str,
    model: &str,
) -> Result<Box<dyn StereoProcessor>> {
    match processor {
        BlockProcessor::Stereo(processor) => Ok(processor),
        BlockProcessor::Mono(_) => Err(anyhow!(
            "chain '{}' {} model '{}' returned mono processing where stereo was required",
            chain.id.0,
            effect_type,
            model
        )),
    }
}

fn required_string(params: &ParameterSet, path: &str) -> Result<String> {
    params
        .get_string(path)
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("missing or invalid string parameter '{}'", path))
}

fn optional_string(params: &ParameterSet, path: &str) -> Option<String> {
    params
        .get_optional_string(path)
        .flatten()
        .map(ToString::to_string)
}

pub fn process_input_f32(
    runtime: &Arc<ChainRuntimeState>,
    input_index: usize,
    data: &[f32],
    input_total_channels: usize,
) {
    let num_frames = data.len() / input_total_channels;
    let mut processing = match runtime.processing.try_lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    let ChainProcessingState {
        input_states,
        tuner_samples,
        mixed_buffer: _,
    } = &mut *processing;

    let input_state = match input_states.get_mut(input_index) {
        Some(s) => s,
        None => return,
    };

    let InputProcessingState {
        input_read_layout,
        processing_layout,
        input_channels,
        blocks,
        frame_buffer,
        fade_in_remaining,
        output_route_indices: _,
    } = input_state;

    let tuner_enabled = blocks.iter().any(block_has_active_tuner);

    frame_buffer.clear();
    let frame_buffer_additional = num_frames.saturating_sub(frame_buffer.capacity());
    if frame_buffer_additional > 0 {
        frame_buffer.reserve(frame_buffer_additional);
    }

    tuner_samples.clear();
    if tuner_enabled {
        let tuner_samples_additional = num_frames.saturating_sub(tuner_samples.capacity());
        if tuner_samples_additional > 0 {
            tuner_samples.reserve(tuner_samples_additional);
        }
    }

    for frame in data.chunks(input_total_channels).take(num_frames) {
        let raw_frame = read_input_frame(*input_read_layout, input_channels, frame);
        // Adapt to processing layout
        let chain_frame = match (*input_read_layout, *processing_layout) {
            (AudioChannelLayout::Mono, AudioChannelLayout::Stereo) => {
                // Mono input -> duplicate to stereo for processing
                let sample = match raw_frame {
                    AudioFrame::Mono(s) => s,
                    _ => unreachable!(),
                };
                AudioFrame::Stereo([sample, sample])
            }
            _ => raw_frame, // layout matches, use as-is
        };
        if tuner_enabled {
            tuner_samples.push(chain_frame.mono_mix());
        }
        frame_buffer.push(chain_frame);
    }

    if tuner_enabled && !tuner_samples.is_empty() {
        // Push samples to shared buffer — UI thread does the detection
        runtime.push_tuner_samples(tuner_samples);
    }

    for block in blocks.iter_mut() {
        process_audio_block(block, frame_buffer.as_mut_slice());
    }

    // Apply fade-in after chain rebuild to avoid clicks/pops
    if *fade_in_remaining > 0 {
        let fade_total = FADE_IN_FRAMES as f32;
        for frame in frame_buffer.iter_mut() {
            if *fade_in_remaining == 0 {
                break;
            }
            let progress = 1.0 - (*fade_in_remaining as f32 / fade_total);
            // Cosine fade for smooth transition
            let gain = 0.5 * (1.0 - (std::f32::consts::PI * progress).cos());
            match frame {
                AudioFrame::Mono(s) => *s *= gain,
                AudioFrame::Stereo([l, r]) => {
                    *l *= gain;
                    *r *= gain;
                }
            }
            *fade_in_remaining -= 1;
        }
    }

    // Collect processed frames and output routing info, then release processing lock
    let processed: Vec<AudioFrame> = input_state.frame_buffer.drain(..).collect();
    let segment_routes = input_state.output_route_indices.clone();
    if input_index == 0 {
        static INPUT0_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let c = INPUT0_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if c % 1000 == 0 {
            log::info!("[process_input] idx=0 frames={} routes={:?} (call #{})", processed.len(), segment_routes, c);
        }
    }
    drop(processing);

    // Mix processed frames into this segment's output routes only.
    // If the queue already has frames (from another input), sum them.
    // Otherwise, push new frames.
    match runtime.output.try_lock() {
        Err(_) => {
            if input_index == 0 {
                static LOCK_FAIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let c = LOCK_FAIL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if c % 100 == 0 {
                    log::warn!("[process_input] idx=0 OUTPUT LOCK FAILED (#{}) — {} frames DROPPED", c, processed.len());
                }
            }
        }
        Ok(mut output) => {
        let route_indices = if segment_routes.is_empty() {
            // Legacy: push to all output routes
            (0..output.output_routes.len()).collect::<Vec<_>>()
        } else {
            segment_routes
        };
        for &route_idx in &route_indices {
            if let Some(route) = output.output_routes.get_mut(route_idx) {
                let existing_len = route.queue.len();
                for (i, &frame) in processed.iter().enumerate() {
                    if i < existing_len {
                        // Sum with existing frame from another input
                        let existing = &mut route.queue[i];
                        *existing = mix_frames(*existing, frame);
                    } else {
                        // No existing frame yet — push as-is
                        route.queue.push_back(frame);
                    }
                }
                trim_output_queue(&mut route.queue);
            }
        }
        }
    }
}

fn block_has_active_tuner(block: &BlockRuntimeNode) -> bool {
    match &block.processor {
        RuntimeProcessor::Tuner(_) => true,
        RuntimeProcessor::Select(select) => select
            .selected_node()
            .map(block_has_active_tuner)
            .unwrap_or(false),
        RuntimeProcessor::Audio(_) | RuntimeProcessor::Bypass => false,
    }
}

/// Simple AMDF pitch detection — runs on UI thread, NOT audio thread.
fn detect_pitch(samples: &[f32]) -> block_util::TunerReading {
    let sample_rate = 44100.0_f32; // TODO: pass actual sample rate
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    if rms < 0.01 || samples.len() < 512 {
        return block_util::TunerReading::default();
    }

    let min_period = (sample_rate / 1000.0) as usize; // ~1000 Hz max
    let max_period = (sample_rate / 65.0) as usize;   // ~65 Hz min (C2)
    let len = samples.len();
    let mut best_period = 0;
    let mut min_diff = f32::MAX;

    for lag in min_period..max_period.min(len / 2) {
        let mut diff = 0.0_f32;
        for i in 0..(len - lag) {
            diff += (samples[i] - samples[i + lag]).abs();
        }
        if diff < min_diff {
            min_diff = diff;
            best_period = lag;
        }
    }

    if best_period == 0 {
        return block_util::TunerReading::default();
    }

    let freq = sample_rate / best_period as f32;
    block_util::TunerReading::from(Some(freq))
}

#[allow(dead_code)]
fn extract_tuner_reading(block: &mut BlockRuntimeNode) -> Option<block_util::TunerReading> {
    match &mut block.processor {
        RuntimeProcessor::Tuner(tuner) => {
            let r = tuner.latest_reading();
            if r.frequency.is_some() { Some(r.clone()) } else { None }
        }
        RuntimeProcessor::Select(select) => {
            select.selected_node_mut().and_then(extract_tuner_reading)
        }
        _ => None,
    }
}

#[allow(dead_code)]
fn process_tuners(block: &mut BlockRuntimeNode, tuner_samples: &[f32]) {
    match &mut block.processor {
        RuntimeProcessor::Tuner(tuner) => {
            if !tuner_samples.is_empty() {
                tuner.process(tuner_samples);
            }
        }
        RuntimeProcessor::Select(select) => {
            if let Some(selected) = select.selected_node_mut() {
                process_tuners(selected, tuner_samples);
            }
        }
        RuntimeProcessor::Audio(_) | RuntimeProcessor::Bypass => {}
    }
}

fn process_audio_block(block: &mut BlockRuntimeNode, frames: &mut [AudioFrame]) {
    // Skip disabled blocks (processor is kept alive for instant re-enable)
    if !block.block_snapshot.enabled {
        return;
    }
    match &mut block.processor {
        RuntimeProcessor::Audio(processor) => {
            processor.process_buffer(frames, &mut block.scratch);
        }
        RuntimeProcessor::Select(select) => {
            if let Some(selected) = select.selected_node_mut() {
                process_audio_block(selected, frames);
            }
        }
        RuntimeProcessor::Tuner(_) | RuntimeProcessor::Bypass => {}
    }
}

pub fn process_output_f32(
    runtime: &Arc<ChainRuntimeState>,
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
) {
    let mut output_state = match runtime.output.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            out.fill(0.0);
            return;
        }
    };
    let route = match output_state.output_routes.get_mut(output_index) {
        Some(r) => r,
        None => {
            out.fill(0.0);
            return;
        }
    };
    let num_frames = out.len() / output_total_channels;
    let queue_len = route.queue.len();
    if queue_len == 0 && output_index > 0 {
        // Log only for non-primary outputs (Insert send) to avoid spam
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let c = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if c % 1000 == 0 {
            log::warn!("[process_output] output_index={} queue EMPTY (call #{})", output_index, c);
        }
    }

    for frame in out.chunks_mut(output_total_channels).take(num_frames) {
        frame.fill(0.0);
        let processed = route
            .queue
            .pop_front()
            .unwrap_or_else(|| silent_frame(route.output_layout));
        write_output_frame(
            processed,
            &route.output_channels,
            frame,
            route.output_mixdown,
        );
    }
}

fn trim_output_queue(queue: &mut VecDeque<AudioFrame>) {
    while queue.len() > MAX_BUFFERED_OUTPUT_FRAMES {
        queue.pop_back();
    }
}

fn read_input_frame(
    input_layout: AudioChannelLayout,
    input_channels: &[usize],
    frame: &[f32],
) -> AudioFrame {
    match input_layout {
        AudioChannelLayout::Mono => AudioFrame::Mono(read_channel(frame, input_channels[0])),
        AudioChannelLayout::Stereo => AudioFrame::Stereo([
            read_channel(frame, input_channels[0]),
            read_channel(frame, input_channels[1]),
        ]),
    }
}

fn read_channel(frame: &[f32], channel_index: usize) -> f32 {
    frame.get(channel_index).copied().unwrap_or(0.0)
}

fn silent_frame(layout: AudioChannelLayout) -> AudioFrame {
    match layout {
        AudioChannelLayout::Mono => AudioFrame::Mono(0.0),
        AudioChannelLayout::Stereo => AudioFrame::Stereo([0.0, 0.0]),
    }
}

/// Sum two audio frames together (for mixing multiple input streams).
fn mix_frames(a: AudioFrame, b: AudioFrame) -> AudioFrame {
    match (a, b) {
        (AudioFrame::Mono(l), AudioFrame::Mono(r)) => AudioFrame::Mono(l + r),
        (AudioFrame::Stereo([l1, r1]), AudioFrame::Stereo([l2, r2])) => {
            AudioFrame::Stereo([l1 + l2, r1 + r2])
        }
        (AudioFrame::Mono(m), AudioFrame::Stereo([l, r])) => {
            AudioFrame::Stereo([m + l, m + r])
        }
        (AudioFrame::Stereo([l, r]), AudioFrame::Mono(m)) => {
            AudioFrame::Stereo([l + m, r + m])
        }
    }
}

/// Soft limiter — transparent below 0dBFS, gentle saturation above.
#[inline]
fn output_limiter(sample: f32) -> f32 {
    if sample.abs() < 0.95 {
        sample
    } else {
        sample.tanh()
    }
}

fn write_output_frame(
    chain_frame: AudioFrame,
    output_channels: &[usize],
    frame: &mut [f32],
    mixdown: ChainOutputMixdown,
) {
    match chain_frame {
        AudioFrame::Mono(sample) => {
            let limited = output_limiter(sample);
            for &channel_index in output_channels {
                if let Some(dst) = frame.get_mut(channel_index) {
                    *dst = limited;
                }
            }
        }
        AudioFrame::Stereo([left, right]) => match output_channels {
            [] => {}
            [channel_index] => {
                if let Some(dst) = frame.get_mut(*channel_index) {
                    *dst = output_limiter(apply_mixdown(mixdown, left, right));
                }
            }
            [left_channel, right_channel, ..] => {
                if let Some(dst) = frame.get_mut(*left_channel) {
                    *dst = output_limiter(left);
                }
                if let Some(dst) = frame.get_mut(*right_channel) {
                    *dst = output_limiter(right);
                }
            }
        },
    }
}

fn apply_mixdown(mixdown: ChainOutputMixdown, left: f32, right: f32) -> f32 {
    match mixdown {
        ChainOutputMixdown::Sum => left + right,
        ChainOutputMixdown::Average => (left + right) * 0.5,
        ChainOutputMixdown::Left => left,
        ChainOutputMixdown::Right => right,
    }
}

#[allow(dead_code)]
fn layout_from_channels(channel_count: usize) -> Result<AudioChannelLayout> {
    match channel_count {
        1 => Ok(AudioChannelLayout::Mono),
        2 => Ok(AudioChannelLayout::Stereo),
        other => Err(anyhow!(
            "only mono and stereo are supported right now; got {} channels",
            other
        )),
    }
}

fn layout_label(layout: AudioChannelLayout) -> &'static str {
    match layout {
        AudioChannelLayout::Mono => "mono",
        AudioChannelLayout::Stereo => "stereo",
    }
}

fn next_block_instance_serial() -> u64 {
    NEXT_BLOCK_INSTANCE_SERIAL.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::{
        build_chain_runtime_state, build_runtime_graph, process_input_f32, process_output_f32,
        update_chain_runtime_state, MAX_BUFFERED_OUTPUT_FRAMES,
    };
    use block_preamp::supported_models as supported_preamp_models;
    use block_cab::{cab_backend_kind, supported_models as supported_cab_models, CabBackendKind};
    use block_delay::supported_models as supported_delay_models;
    use block_dyn::compressor_supported_models;
    use block_reverb::supported_models as supported_reverb_models;
    use block_util::supported_models as supported_tuner_models;
    use domain::ids::{BlockId, DeviceId, ChainId};
    use domain::value_objects::ParameterValue;
    use project::block::{
        AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry, SelectBlock, schema_for_block_model,
    };
    use project::param::ParameterSet;
    use project::project::Project;
    use project::chain::{Chain, ChainInputMode, ChainOutputMode};
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn runtime_graph_builds_for_chain_with_cab_block() {
        let (model, params) = any_ir_cab_defaults();

        let project = Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: ChainId("chain:0".into()),
                description: Some("Cab test".into()),
                instrument: "electric_guitar".to_string(),
                enabled: true,
                blocks: vec![AudioBlock {
                    id: BlockId("chain:0:block:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock {
                        effect_type: "cab".to_string(),
                        model,
                        params,
                    }),
                }],
            }],
        };

        let runtime = build_runtime_graph(
            &project,
            &HashMap::from([(ChainId("chain:0".into()), 48_000.0)]),
        )
        .expect("runtime graph should build");
        assert_eq!(runtime.chains.len(), 1);
    }

    #[test]
    fn runtime_graph_rejects_chain_when_runtime_sample_rate_does_not_match_ir() {
        let (model, params) = any_ir_cab_defaults();

        let project = Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: ChainId("chain:0".into()),
                description: Some("Cab test".into()),
                instrument: "electric_guitar".to_string(),
                enabled: true,
                blocks: vec![AudioBlock {
                    id: BlockId("chain:0:block:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock {
                        effect_type: "cab".to_string(),
                        model,
                        params,
                    }),
                }],
            }],
        };

        let error = match build_runtime_graph(
            &project,
            &HashMap::from([(ChainId("chain:0".into()), 44_100.0)]),
        ) {
            Ok(_) => panic!("runtime graph should reject mismatched IR sample rate"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("sample_rate"));
    }

    #[test]
    fn update_chain_runtime_state_preserves_unchanged_block_instances() {
        let mut chain = tuner_track(
            "chain:0",
            vec![
                tuner_block("chain:0:block:a", 440.0),
                tuner_block("chain:0:block:b", 445.0),
            ],
        );

        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));
        let original_serials = {
            let locked = runtime.processing.lock().expect("runtime poisoned");
            locked
                .input_states[0]
                .blocks
                .iter()
                .map(|block| block.instance_serial)
                .collect::<Vec<_>>()
        };

        if let AudioBlockKind::Core(core) = &mut chain.blocks[1].kind {
            core.params
                .insert("reference_hz", ParameterValue::Float(432.0));
        }

        update_chain_runtime_state(&runtime, &chain, 48_000.0, false)
            .expect("runtime update should succeed");

        let updated_serials = {
            let locked = runtime.processing.lock().expect("runtime poisoned");
            locked
                .input_states[0]
                .blocks
                .iter()
                .map(|block| block.instance_serial)
                .collect::<Vec<_>>()
        };

        assert_eq!(updated_serials[0], original_serials[0]);
        assert_ne!(updated_serials[1], original_serials[1]);
    }

    #[test]
    fn update_chain_runtime_state_preserves_block_identity_when_reordered() {
        let mut chain = tuner_track(
            "chain:0",
            vec![
                tuner_block("chain:0:block:a", 440.0),
                tuner_block("chain:0:block:b", 445.0),
            ],
        );

        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));
        let original_by_block_id = {
            let locked = runtime.processing.lock().expect("runtime poisoned");
            locked
                .input_states[0]
                .blocks
                .iter()
                .map(|block| (block.block_id.clone(), block.instance_serial))
                .collect::<HashMap<_, _>>()
        };

        chain.blocks.swap(0, 1);

        update_chain_runtime_state(&runtime, &chain, 48_000.0, false)
            .expect("runtime update should succeed");

        let reordered = runtime.processing.lock().expect("runtime poisoned");
        assert_eq!(reordered.input_states[0].blocks.len(), 2);
        for block in &reordered.input_states[0].blocks {
            assert_eq!(
                Some(&block.instance_serial),
                original_by_block_id.get(&block.block_id)
            );
        }
    }

    #[test]
    fn process_input_limits_buffered_output_frames() {
        let chain = tuner_track("chain:0", Vec::new());
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));
        let total_frames = MAX_BUFFERED_OUTPUT_FRAMES + 64;
        let input = vec![0.25f32; total_frames];

        process_input_f32(&runtime, 0, &input, 1);

        let output = runtime.output.lock().expect("runtime poisoned");
        assert_eq!(output.output_routes[0].queue.len(), MAX_BUFFERED_OUTPUT_FRAMES);
    }

    #[test]
    fn process_output_drains_buffered_frames() {
        let chain = tuner_track("chain:0", Vec::new());
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));

        process_input_f32(&runtime, 0, &[0.25, 0.5, 0.75, 1.0], 1);

        let mut out = vec![0.0f32; 4];
        process_output_f32(&runtime, 0, &mut out, 1);

        assert_eq!(out, vec![0.25, 0.5, 0.75, 1.0]);
        let output = runtime.output.lock().expect("runtime poisoned");
        assert!(output.output_routes[0].queue.is_empty());
    }

    #[test]
    fn dual_mono_chain_does_not_leak_left_into_right() {
        let chain = Chain {
            id: ChainId("chain:stereo".into()),
            description: Some("Stereo isolation".into()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("chain:stereo:input:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            name: "Input 1".to_string(),
                            device_id: DeviceId("input-device".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0, 1],
                        }],
                    }),
                },
                compressor_block("chain:stereo:block:0"),
                preamp_block("chain:stereo:block:1"),
                native_cab_block("chain:stereo:block:2"),
                reverb_block("chain:stereo:block:3"),
                AudioBlock {
                    id: BlockId("chain:stereo:output:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            name: "Output 1".to_string(),
                            device_id: DeviceId("output-device".into()),
                            mode: ChainOutputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
            ],
        };
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));

        let mut input = vec![0.0f32; 256 * 2];
        for frame in input.chunks_mut(2) {
            frame[0] = 0.25;
            frame[1] = 0.0;
        }
        process_input_f32(&runtime, 0, &input, 2);

        let mut output = vec![0.0f32; input.len()];
        process_output_f32(&runtime, 0, &mut output, 2);

        let right_peak = output
            .chunks_exact(2)
            .map(|frame| frame[1].abs())
            .fold(0.0f32, f32::max);
        assert!(
            right_peak <= 1.0e-6,
            "dual-mono chain leaked signal into right channel: peak={right_peak}"
        );
    }

    #[test]
    fn asset_backed_dual_mono_chain_does_not_leak_left_into_right() {
        let chain = Chain {
            id: ChainId("chain:asset-backed".into()),
            description: Some("Stereo isolation asset-backed".into()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("chain:asset-backed:input:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            name: "Input 1".to_string(),
                            device_id: DeviceId("input-device".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0, 1],
                        }],
                    }),
                },
                marshall_preamp_block("chain:asset-backed:block:0"),
                ir_cab_block("chain:asset-backed:block:1"),
                reverb_block("chain:asset-backed:block:2"),
                AudioBlock {
                    id: BlockId("chain:asset-backed:output:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            name: "Output 1".to_string(),
                            device_id: DeviceId("output-device".into()),
                            mode: ChainOutputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
            ],
        };
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));

        let mut input = vec![0.0f32; 256 * 2];
        for frame in input.chunks_mut(2) {
            frame[0] = 0.25;
            frame[1] = 0.0;
        }
        process_input_f32(&runtime, 0, &input, 2);

        let mut output = vec![0.0f32; input.len()];
        process_output_f32(&runtime, 0, &mut output, 2);

        let right_peak = output
            .chunks_exact(2)
            .map(|frame| frame[1].abs())
            .fold(0.0f32, f32::max);
        assert!(
            right_peak <= 1.0e-6,
            "asset-backed dual-mono chain leaked signal into right channel: peak={right_peak}"
        );
    }

    #[test]
    fn select_block_builds_for_generic_delay_options() {
        let chain = select_delay_chain("chain:select", "delay_a");

        let runtime =
            build_chain_runtime_state(&chain, 48_000.0).expect("select delay chain should build");

        let locked = runtime.processing.lock().expect("runtime poisoned");
        assert_eq!(locked.input_states[0].blocks.len(), 1);
    }

    #[test]
    fn update_chain_runtime_state_preserves_select_instance_when_switching_active_option() {
        let mut chain = select_delay_chain("chain:select", "delay_a");
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));
        let original_serial = {
            let locked = runtime.processing.lock().expect("runtime poisoned");
            locked.input_states[0].blocks[0].instance_serial
        };

        if let AudioBlockKind::Select(select) = &mut chain.blocks[0].kind {
            select.selected_block_id = BlockId("chain:select:block:0::delay_b".into());
        }

        update_chain_runtime_state(&runtime, &chain, 48_000.0, false)
            .expect("runtime update should succeed when switching select option");

        let updated_serial = {
            let locked = runtime.processing.lock().expect("runtime poisoned");
            locked.input_states[0].blocks[0].instance_serial
        };

        assert_eq!(updated_serial, original_serial);
    }

    fn tuner_track(chain_id: &str, blocks: Vec<AudioBlock>) -> Chain {
        Chain {
            id: ChainId(chain_id.into()),
            description: Some("Tuner chain".into()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks,
        }
    }

    fn tuner_block(block_id: &str, reference_hz: f32) -> AudioBlock {
        let tuner_model = supported_tuner_models()
            .first()
            .expect("block-util must expose at least one tuner model")
            .to_string();
        let mut params = ParameterSet::default();
        params.insert("reference_hz", ParameterValue::Float(reference_hz));
        AudioBlock {
            id: BlockId(block_id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "utility".to_string(),
                model: tuner_model,
                params,
            }),
        }
    }

    fn any_ir_cab_defaults() -> (String, ParameterSet) {
        let model = supported_cab_models()
            .iter()
            .find(|model| {
                matches!(
                    cab_backend_kind(model).expect("cab backend should resolve"),
                    CabBackendKind::Ir
                )
            })
            .expect("block-cab must expose at least one IR-backed model")
            .to_string();
        let schema = block_cab::cab_model_schema(&model).expect("cab schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("cab defaults should normalize");
        (model, params)
    }

    fn normalized_defaults(effect_type: &str, model: &str) -> ParameterSet {
        let schema =
            schema_for_block_model(effect_type, model).expect("schema should exist for test model");
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    fn compressor_block(block_id: &str) -> AudioBlock {
        let model = compressor_supported_models()
            .first()
            .expect("block-dyn must expose at least one compressor")
            .to_string();
        AudioBlock {
            id: BlockId(block_id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "dynamics".to_string(),
                params: normalized_defaults("dynamics", &model),
                model,
            }),
        }
    }

    fn native_cab_block(block_id: &str) -> AudioBlock {
        let model = supported_cab_models()
            .iter()
            .find(|model| matches!(cab_backend_kind(model).expect("cab backend"), CabBackendKind::Native))
            .expect("block-cab must expose at least one native model")
            .to_string();
        AudioBlock {
            id: BlockId(block_id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "cab".to_string(),
                params: normalized_defaults("cab", &model),
                model,
            }),
        }
    }

    fn preamp_block(block_id: &str) -> AudioBlock {
        let model = supported_preamp_models()
            .iter()
            .find(|model| !model.contains("marshall_jcm_800"))
            .or_else(|| supported_preamp_models().first())
            .expect("block-preamp must expose at least one model")
            .to_string();
        AudioBlock {
            id: BlockId(block_id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "preamp".to_string(),
                params: normalized_defaults("preamp", &model),
                model,
            }),
        }
    }

    fn marshall_preamp_block(block_id: &str) -> AudioBlock {
        let model = "marshall_jcm_800_2203".to_string();
        AudioBlock {
            id: BlockId(block_id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "preamp".to_string(),
                params: normalized_defaults("preamp", &model),
                model,
            }),
        }
    }

    fn ir_cab_block(block_id: &str) -> AudioBlock {
        let model = supported_cab_models()
            .iter()
            .find(|model| matches!(cab_backend_kind(model).expect("cab backend"), CabBackendKind::Ir))
            .expect("block-cab must expose at least one IR model")
            .to_string();
        AudioBlock {
            id: BlockId(block_id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "cab".to_string(),
                params: normalized_defaults("cab", &model),
                model,
            }),
        }
    }

    fn reverb_block(block_id: &str) -> AudioBlock {
        let model = supported_reverb_models()
            .first()
            .expect("block-reverb must expose at least one model")
            .to_string();
        AudioBlock {
            id: BlockId(block_id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "reverb".to_string(),
                params: normalized_defaults("reverb", &model),
                model,
            }),
        }
    }

    fn select_delay_chain(id: &str, selected_option: &str) -> Chain {
        let models = supported_delay_models();
        let first_model = models
            .first()
            .expect("block-delay must expose at least one model");
        let second_model = models.get(1).unwrap_or(first_model);

        Chain {
            id: ChainId(id.into()),
            description: Some("Delay select".into()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![AudioBlock {
                id: BlockId(format!("{id}:block:0")),
                enabled: true,
                kind: AudioBlockKind::Select(SelectBlock {
                    selected_block_id: BlockId(format!("{id}:block:0::{selected_option}")),
                    options: vec![
                        delay_block(format!("{id}:block:0::delay_a"), first_model, 120.0),
                        delay_block(format!("{id}:block:0::delay_b"), second_model, 240.0),
                    ],
                }),
            }],
        }
    }

    fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
        let mut params = normalized_defaults("delay", model);
        params.insert("time_ms", ParameterValue::Float(time_ms));
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "delay".to_string(),
                model: model.to_string(),
                params,
            }),
        }
    }
}
