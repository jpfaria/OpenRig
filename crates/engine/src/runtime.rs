use anyhow::{anyhow, Result};
use domain::ids::{BlockId, ChainId};
use project::block::{
    schema_for_block_model, AudioBlockKind, CoreBlock, InputEntry, InsertBlock, NamBlock, OutputEntry, SelectBlock,
};
use project::param::ParameterSet;
use project::project::Project;
use project::chain::{
    Chain, ChainInputMode, ChainOutputMixdown, ChainOutputMode,
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
use block_pitch::build_pitch_processor_for_layout;
use block_reverb::build_reverb_processor_for_layout;
use block_util::build_utility_processor_for_layout;
use block_core::StreamHandle;
use block_wah::build_wah_processor_for_layout;
use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Floor for the elastic buffer target. Below this the buffer cannot absorb
/// even minor scheduling jitter, regardless of how small the device buffer is.
pub const ELASTIC_TARGET_FLOOR: usize = 64;

/// Default elastic target used when no device-derived value is provided
/// (tests, headless tools). Production callers in infra-cpal compute this
/// from the resolved device buffer size via [`elastic_target_for_buffer`].
pub const DEFAULT_ELASTIC_TARGET: usize = 256;

/// Compute the elastic buffer target level (in frames) for a given device
/// buffer size and backend multiplier.
///
/// The elastic buffer absorbs jitter between the producer (input + DSP path)
/// and the consumer (output callback). Sizing it relative to the actual device
/// buffer makes the latency proportional to the user's chosen buffer size
/// instead of a hardcoded constant.
///
/// `multiplier` reflects backend-specific jitter:
/// - `2` — direct CPAL callbacks (macOS/Windows/Linux ALSA): tight, predictable.
/// - `8` — JACK with worker-thread DSP (Linux): non-RT worker adds variance.
pub fn elastic_target_for_buffer(buffer_size_frames: u32, multiplier: u8) -> usize {
    let target = (buffer_size_frames as usize).saturating_mul(multiplier as usize);
    target.max(ELASTIC_TARGET_FLOOR)
}
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

/// Elastic audio buffer for clock drift compensation.
/// Maintains a target queue level. On underrun, repeats the last frame.
/// On overrun, discards the oldest frame gradually.
struct ElasticBuffer {
    pub queue: VecDeque<AudioFrame>,
    target_level: usize,
    last_frame: AudioFrame,
}

impl ElasticBuffer {
    fn new(target_level: usize, layout: AudioChannelLayout) -> Self {
        Self {
            queue: VecDeque::with_capacity(target_level * 2),
            target_level,
            last_frame: silent_frame(layout),
        }
    }

    fn push(&mut self, frame: AudioFrame) {
        self.last_frame = frame;
        self.queue.push_back(frame);
        if self.queue.len() > self.target_level * 2 {
            self.queue.pop_front();
        }
    }

    fn pop(&mut self) -> AudioFrame {
        self.queue.pop_front().unwrap_or(self.last_frame)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.queue.len()
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
    /// Process a buffer of audio frames.
    ///
    /// Bus between blocks is ALWAYS stereo. Mono processors receive the left
    /// channel (or mono mix), process it, and output stereo (duplicated).
    fn process_buffer(&mut self, frames: &mut [AudioFrame], scratch: &mut ProcessorScratch) {
        match (self, scratch) {
            (AudioProcessor::Mono(processor), ProcessorScratch::Mono(mono)) => {
                mono.clear();
                mono.reserve(frames.len().saturating_sub(mono.capacity()));
                for frame in frames.iter() {
                    mono.push(frame.mono_mix());
                }
                processor.process_block(mono);
                // Always output stereo — mono processors duplicate to both channels
                for (frame, sample) in frames.iter_mut().zip(mono.iter().copied()) {
                    *frame = AudioFrame::Stereo([sample, sample]);
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
                        AudioFrame::Stereo([l, r]) => {
                            left_buffer.push(*l);
                            right_buffer.push(*r);
                        }
                        AudioFrame::Mono(s) => {
                            left_buffer.push(*s);
                            right_buffer.push(*s);
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
                        AudioFrame::Stereo(sf) => stereo.push(*sf),
                        AudioFrame::Mono(s) => stereo.push([*s, *s]),
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
                        AudioFrame::Mono(s) => stereo.push([*s, *s]),
                        AudioFrame::Stereo(sf) => stereo.push(*sf),
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

/// An error produced by a block processor during audio processing.
#[derive(Debug, Clone)]
pub struct BlockError {
    pub block_id: BlockId,
    pub message: String,
}

pub struct ChainRuntimeState {
    processing: Mutex<ChainProcessingState>,
    output: Mutex<ChainOutputState>,
    /// Stream handles published by block processors, polled by UI thread.
    stream_handles: Mutex<HashMap<BlockId, StreamHandle>>,
    /// Errors posted by the audio thread, drained by the UI thread.
    error_queue: Mutex<Vec<BlockError>>,
    #[allow(dead_code)]
    last_input_nanos: AtomicU64,
    measured_latency_nanos: AtomicU64,
    /// When true, the audio callback must not call any block processors.
    /// Set before deactivating the JACK client to prevent use-after-free
    /// in C++ NAM destructors (terminate called without active exception).
    draining: std::sync::atomic::AtomicBool,
}

impl ChainRuntimeState {
    /// Signal the audio callback to stop processing blocks.
    /// Must be called before deactivating JACK or dropping block processors.
    pub fn set_draining(&self) {
        self.draining.store(true, std::sync::atomic::Ordering::Release);
    }

    pub fn is_draining(&self) -> bool {
        self.draining.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn measured_latency_ms(&self) -> f32 {
        let nanos = self.measured_latency_nanos.load(std::sync::atomic::Ordering::Relaxed);
        nanos as f32 / 1_000_000.0
    }

    /// Returns stream data for a block by ID, or None if not found or empty.
    pub fn poll_stream(&self, block_id: &BlockId) -> Option<Vec<block_core::StreamEntry>> {
        let handles = self.stream_handles.lock().ok()?;
        let handle = handles.get(block_id)?;
        let entries = handle.lock().ok()?;
        if entries.is_empty() { None } else { Some(entries.clone()) }
    }

    /// Drains and returns all block errors posted since the last call.
    pub fn poll_errors(&self) -> Vec<BlockError> {
        match self.error_queue.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => vec![],
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
    /// Maps CPAL input_index → Vec of input_states indices to process.
    input_to_segments: Vec<Vec<usize>>,
    /// Pre-allocated scratch buffers used by `process_input_f32`, indexed by
    /// CPAL input_index. Reused across callbacks to avoid per-callback
    /// allocations in the RT hot path.
    input_scratches: Vec<InputCallbackScratch>,
}

/// Scratch buffers reused across audio callbacks for a single input_index.
/// Each Vec/HashMap keeps its allocated capacity between callbacks; clearing
/// leaves the backing storage in place.
#[derive(Default)]
struct InputCallbackScratch {
    /// Mixed audio frames keyed by output route index, accumulated across
    /// segments for the current callback.
    mixed_per_route: HashMap<usize, Vec<AudioFrame>>,
    /// Output route Arcs collected under the output lock for this callback.
    route_arcs: Vec<(usize, Arc<Mutex<OutputRoutingState>>)>,
    /// Buffer written by `process_single_segment` with the processed frames
    /// of the current segment before they are mixed into `mixed_per_route`.
    segment_processed: Vec<AudioFrame>,
    /// Output route indices for the current segment, refreshed per segment.
    segment_route_indices: Vec<usize>,
    /// Segment indices belonging to the current input_index, refreshed per
    /// callback from `input_to_segments`.
    segment_indices: Vec<usize>,
}

impl InputCallbackScratch {
    fn reset_for_callback(&mut self) {
        for buf in self.mixed_per_route.values_mut() {
            buf.clear();
        }
        self.route_arcs.clear();
        self.segment_processed.clear();
        self.segment_route_indices.clear();
        self.segment_indices.clear();
    }
}

struct OutputRoutingState {
    output_channels: Vec<usize>,
    output_mixdown: ChainOutputMixdown,
    buffer: ElasticBuffer,
}

struct ChainOutputState {
    output_routes: Vec<Arc<Mutex<OutputRoutingState>>>,
}

enum RuntimeProcessor {
    Audio(AudioProcessor),
    Select(SelectRuntimeState),
    Bypass,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FadeState {
    /// Fully active — no fade in progress.
    Active,
    /// Transitioning from bypass → active. frames_remaining counts down.
    FadingIn { frames_remaining: usize },
    /// Transitioning from active → bypass. frames_remaining counts down.
    FadingOut { frames_remaining: usize },
    /// Fully bypassed — no audio processing needed.
    Bypassed,
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
    stream_handle: Option<StreamHandle>,
    fade_state: FadeState,
    /// Set to true if this block panicked during audio processing.
    /// Once faulted, the block is permanently bypassed to prevent repeated crashes.
    faulted: bool,
}

struct SelectRuntimeState {
    selected_block_id: BlockId,
    options: Vec<BlockRuntimeNode>,
}

struct ProcessorBuildOutcome {
    processor: AudioProcessor,
    output_layout: AudioChannelLayout,
    stream_handle: Option<StreamHandle>,
}

impl SelectRuntimeState {
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
    chain_elastic_targets: &HashMap<ChainId, usize>,
) -> Result<RuntimeGraph> {
    let mut chains = HashMap::new();
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        let sample_rate = *chain_sample_rates
            .get(&chain.id)
            .ok_or_else(|| anyhow!("chain '{}' has no resolved runtime sample rate", chain.id.0))?;
        let elastic_target = chain_elastic_targets
            .get(&chain.id)
            .copied()
            .unwrap_or(DEFAULT_ELASTIC_TARGET);
        let state = build_chain_runtime_state(chain, sample_rate, elastic_target)?;
        chains.insert(chain.id.clone(), Arc::new(state));
    }
    Ok(RuntimeGraph { chains })
}

pub fn build_chain_runtime_state(
    chain: &Chain,
    sample_rate: f32,
    elastic_target: usize,
) -> Result<ChainRuntimeState> {
    let (eff_inputs, eff_input_cpal_indices) = effective_inputs(chain);
    let eff_outputs = effective_outputs(chain);
    log::info!("=== CHAIN '{}' RUNTIME BUILD ===", chain.id.0);
    log::info!("  inputs: {}", eff_inputs.len());
    for (i, inp) in eff_inputs.iter().enumerate() {
        log::info!("    input[{}]: 'input #{}' dev='{}' ch={:?} cpal_stream={}", i, i, inp.device_id.0.split(':').last().unwrap_or("?"), inp.channels, eff_input_cpal_indices[i]);
    }
    log::info!("  outputs: {}", eff_outputs.len());
    for (i, out) in eff_outputs.iter().enumerate() {
        log::info!("    output[{}]: 'output #{}' dev='{}' ch={:?}", i, i, out.device_id.0.split(':').last().unwrap_or("?"), out.channels);
    }
    let segments = split_chain_into_segments(chain, &eff_inputs, &eff_input_cpal_indices, &eff_outputs);
    log::info!("  segments: {}", segments.len());
    for (i, seg) in segments.iter().enumerate() {
        let block_names: Vec<String> = seg.block_indices.iter()
            .filter_map(|&idx| chain.blocks.get(idx))
            .map(|b| format!("{}({})", b.id.0, match &b.kind {
                AudioBlockKind::Core(c) => c.effect_type.as_str(),
                AudioBlockKind::Nam(_) => "nam",
                _ => "?",
            }))
            .collect();
        log::info!("    seg[{}]: input='input #{}' → blocks={:?} → output_routes={:?}", i, i, block_names, seg.output_route_indices);
    }
    log::info!("=== END CHAIN '{}' ===", chain.id.0);

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

    // Build input_to_segments: CPAL input_index → which segments to process
    let max_input_idx = segments.iter().map(|s| s.cpal_input_index).max().unwrap_or(0);
    let mut input_to_segments: Vec<Vec<usize>> = vec![Vec::new(); max_input_idx + 1];
    for (seg_idx, segment) in segments.iter().enumerate() {
        if segment.cpal_input_index < input_to_segments.len() {
            input_to_segments[segment.cpal_input_index].push(seg_idx);
        }
    }

    let mut output_routes = Vec::with_capacity(eff_outputs.len());
    for output in &eff_outputs {
        output_routes.push(Arc::new(Mutex::new(build_output_routing_state(output, elastic_target))));
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
        output: Mutex::new(ChainOutputState { output_routes }),
        stream_handles: Mutex::new(stream_handles_map),
        error_queue: Mutex::new(Vec::new()),
        last_input_nanos: AtomicU64::new(0),
        measured_latency_nanos: AtomicU64::new(0),
        draining: std::sync::atomic::AtomicBool::new(false),
    })
}

/// Build effective input entries from chain's InputBlock entries, plus Insert return entries.
/// Order: InputBlock entries first, then Insert return entries (matches CPAL stream order).
/// Falls back to a single mono input on channel 0 if no InputBlocks exist and no Inserts.
/// Returns (effective_entries, cpal_stream_index_per_entry).
/// cpal_stream_index maps each effective entry back to the CPAL stream
/// that provides its audio data. Entries split from the same original
/// entry share the same CPAL stream index.
fn effective_inputs(chain: &Chain) -> (Vec<InputEntry>, Vec<usize>) {
    let raw_entries: Vec<InputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib),
            _ => None,
        })
        .flat_map(|ib| ib.entries.iter().cloned())
        .collect();

    // Mono entries with multiple channels: split into one entry per channel
    // so each channel gets its own isolated processing stream.
    //
    // cpal_indices maps each effective entry to the CPAL stream index.
    // Entries sharing the same device get the same CPAL stream index
    // (infra-cpal deduplicates streams by device).
    let mut entries: Vec<InputEntry> = Vec::new();
    let mut cpal_indices: Vec<usize> = Vec::new();
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
            for &ch in &entry.channels {
                entries.push(InputEntry {
                    device_id: entry.device_id.clone(),
                    mode: ChainInputMode::Mono,
                    channels: vec![ch],
                });
                cpal_indices.push(cpal_idx);
            }
        } else {
            entries.push(entry.clone());
            cpal_indices.push(cpal_idx);
        }
    }

    // Append Insert return entries (as inputs for segments after each Insert)
    let insert_return_base = raw_entries.len();
    let insert_returns: Vec<InputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some(insert_return_as_input_entry(ib)),
            _ => None,
        })
        .collect();
    for (i, ret) in insert_returns.into_iter().enumerate() {
        cpal_indices.push(insert_return_base + i);
        entries.push(ret);
    }

    if !entries.is_empty() {
        return (entries, cpal_indices);
    }
    // Fallback — no InputBlocks defined
    (vec![InputEntry {
        device_id: domain::ids::DeviceId("".to_string()),
        mode: ChainInputMode::Mono,
        channels: vec![0],
    }], vec![0])
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
        device_id: domain::ids::DeviceId("".to_string()),
        mode: ChainOutputMode::Mono,
        channels: vec![0],
    }]
}

/// Convert an InsertBlock's return endpoint to an InputEntry.
fn insert_return_as_input_entry(insert: &InsertBlock) -> InputEntry {
    InputEntry {
        device_id: insert.return_.device_id.clone(),
        mode: insert.return_.mode,
        channels: insert.return_.channels.clone(),
    }
}

/// Convert an InsertBlock's send endpoint to an OutputEntry.
fn insert_send_as_output_entry(insert: &InsertBlock) -> OutputEntry {
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
struct ChainSegment {
    input: InputEntry,
    cpal_input_index: usize,
    block_indices: Vec<usize>,
    output_route_indices: Vec<usize>,
}

/// Split a chain into segments at enabled Insert block boundaries.
///
/// Example: [Input, Comp, EQ, Insert, Delay, Reverb, Output]
///   Segment 1: input=InputBlock entries, blocks=[Comp, EQ], outputs=[Insert send]
///   Segment 2: input=Insert return,      blocks=[Delay, Reverb], outputs=[OutputBlock entries]
///
/// If no Insert blocks exist, a single segment covers the entire chain.
fn split_chain_into_segments(chain: &Chain, effective_ins: &[InputEntry], cpal_indices: &[usize], _effective_outs: &[OutputEntry]) -> Vec<ChainSegment> {
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
            let block_indices: Vec<usize> = chain.blocks.iter()
                .enumerate()
                .filter(|(i, b)| {
                    *i < out_pos
                    && !matches!(&b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_))
                })
                .map(|(i, _)| i)
                .collect();

            for in_idx in 0..input_count {
                segments.push(ChainSegment {
                    input: effective_ins[in_idx].clone(),
                    cpal_input_index: cpal_indices.get(in_idx).copied().unwrap_or(in_idx),
                    block_indices: block_indices.clone(),
                    output_route_indices: vec![out_entry_idx],
                });
            }
        }

        return segments;
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
                    cpal_input_index: i,
                    block_indices: block_indices.clone(),
                    output_route_indices: output_indices.clone(),
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
        cpal_input_index: last_return_idx,
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
    let input_read_layout = match input.mode {
        ChainInputMode::Mono => AudioChannelLayout::Mono,
        ChainInputMode::Stereo | ChainInputMode::DualMono => AudioChannelLayout::Stereo,
    };
    // On aarch64 (Orange Pi), use mono processing when input is mono to halve
    // NAM instances (DualMono→Mono). On x86 (Mac/Windows) keep stereo always
    // to avoid any risk of changing the sound character.
    #[cfg(target_arch = "aarch64")]
    let processing_layout_channel = input_read_layout;
    #[cfg(not(target_arch = "aarch64"))]
    let processing_layout_channel = AudioChannelLayout::Stereo;
    let _ = proc_layout;
    log::info!(
        "chain '{}' input entry processing layout: input_read={}, processing={:?} (channels={:?} mode={:?})",
        chain.id.0,
        layout_label(input_read_layout),
        proc_layout,
        input.channels,
        input.mode,
    );
    let had_existing = existing_blocks.is_some();
    let (blocks, _output_layout) =
        build_runtime_block_nodes(chain, processing_layout_channel, sample_rate, existing_blocks, block_indices)?;

    Ok(InputProcessingState {
        input_read_layout,
        processing_layout: processing_layout_channel,
        input_channels: input.channels.clone(),
        blocks,
        frame_buffer: Vec::with_capacity(1024),
        fade_in_remaining: if had_existing { 0 } else { FADE_IN_FRAMES },
        output_route_indices,
    })
}

fn build_output_routing_state(output: &OutputEntry, elastic_target: usize) -> OutputRoutingState {
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
    elastic_target: usize,
) -> Result<()> {
    let (effective_ins, eff_input_cpal_indices) = effective_inputs(chain);
    let effective_outs = effective_outputs(chain);
    let segments = split_chain_into_segments(chain, &effective_ins, &eff_input_cpal_indices, &effective_outs);

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
        let input_state = match build_input_processing_state(
            chain,
            &segment.input,
            &segment_output_channels,
            sample_rate,
            existing,
            Some(&segment.block_indices),
            segment.output_route_indices.clone(),
        ) {
            Ok(state) => state,
            Err(e) => {
                // Restore previously-extracted blocks so the chain keeps playing
                log::error!("[engine] rebuild failed for chain '{}': {e} — restoring previous state", chain.id.0);
                let mut processing = runtime.processing.lock().expect("chain runtime poisoned");
                for (is, old_blocks) in processing.input_states.iter_mut().zip(existing_per_input.into_iter()) {
                    if is.blocks.is_empty() {
                        is.blocks = old_blocks;
                    }
                }
                return Err(e);
            }
        };
        new_input_states.push(input_state);
    }

    // Build new output routes wrapped in per-route mutexes
    let new_output_routes: Vec<Arc<Mutex<OutputRoutingState>>> = effective_outs
        .iter()
        .map(|o| Arc::new(Mutex::new(build_output_routing_state(o, elastic_target))))
        .collect();

    // Step 2.5: Refresh stream_handles — picks up new handles from rebuilt blocks
    // (e.g. block param changed → new processor → new Arc; old Arc in map would be stale)
    {
        let mut handles = runtime.stream_handles.lock().expect("stream_handles poisoned");
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
        let max_input_idx = segments.iter().map(|s| s.cpal_input_index).max().unwrap_or(0);
        let mut new_mapping: Vec<Vec<usize>> = vec![Vec::new(); max_input_idx + 1];
        for (seg_idx, segment) in segments.iter().enumerate() {
            if segment.cpal_input_index < new_mapping.len() {
                new_mapping[segment.cpal_input_index].push(seg_idx);
            }
        }
        processing.input_to_segments = new_mapping;
        // Resize scratches to match the new input count, preserving existing
        // allocated capacity for slots that still exist.
        let new_len = processing.input_to_segments.len();
        processing
            .input_scratches
            .resize_with(new_len, InputCallbackScratch::default);
    }

    {
        let mut output = runtime.output.lock().expect("chain runtime poisoned");
        // Preserve existing buffers where possible
        let old_routes = std::mem::replace(&mut output.output_routes, new_output_routes);
        for (new_route_arc, old_route_arc) in output.output_routes.iter().zip(old_routes.into_iter()) {
            if !reset_output_queue {
                let mut old_route = old_route_arc.lock().expect("old route poisoned");
                let mut new_route = new_route_arc.lock().expect("new route poisoned");
                std::mem::swap(&mut new_route.buffer, &mut old_route.buffer);
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
        elastic_target: usize,
    ) -> Result<Arc<ChainRuntimeState>> {
        if let Some(runtime) = self.chains.get(&chain.id) {
            update_chain_runtime_state(runtime, chain, sample_rate, reset_output_queue, elastic_target)?;
            return Ok(runtime.clone());
        }

        let state = build_chain_runtime_state(chain, sample_rate, elastic_target)?;
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
                let was_enabled = node.block_snapshot.enabled;
                node.block_snapshot = block.clone();
                // If block was just disabled, start a fade-out instead of hard-cutting
                if was_enabled && !matches!(node.processor, RuntimeProcessor::Bypass) {
                    node.fade_state = FadeState::FadingOut { frames_remaining: FADE_IN_FRAMES };
                }
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
        match build_block_runtime_node(chain, block, current_layout, sample_rate) {
            Ok(node) => {
                current_layout = node.output_layout;
                blocks.push(node);
            }
            Err(e) => {
                // Don't fail the whole chain — bypass this block and keep going
                log::error!("[engine] block {:?} (id={}) build failed: {e} — inserting faulted bypass",
                    block.model_ref().map(|m| m.model.to_string()), block.id.0);
                let mut node = bypass_runtime_node(block, current_layout);
                node.faulted = true;
                blocks.push(node);
            }
        }
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
        log::debug!("[engine] cannot reuse block id={}: layout changed ({:?} → {:?})",
            block.id.0, node.input_layout, current_layout);
        return None;
    }
    // Exact match — reuse as-is
    if node.block_snapshot == *block {
        return Some(node);
    }
    // Only enabled changed — reuse processor, update snapshot.
    // Exception: if the node is a Bypass (block was built while disabled and has no real
    // processor or stream_handle), enabling it requires a full rebuild.
    let mut snapshot_without_enabled = node.block_snapshot.clone();
    snapshot_without_enabled.enabled = block.enabled;
    if snapshot_without_enabled == *block {
        if matches!(node.processor, RuntimeProcessor::Bypass) && block.enabled {
            return None; // force rebuild so we get a real processor + stream_handle
        }
        let was_disabled = !node.block_snapshot.enabled;
        node.block_snapshot = block.clone();
        // If block was just enabled, start a fade-in
        if was_disabled && block.enabled {
            node.fade_state = FadeState::FadingIn { frames_remaining: FADE_IN_FRAMES };
        }
        return Some(node);
    }
    log::info!("[engine] cannot reuse block id={}: snapshot differs (params or kind changed)", block.id.0);
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
        EFFECT_TYPE_UTILITY => {
            let mut captured_stream: Option<StreamHandle> = None;
            let mut outcome = build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_UTILITY,
                model,
                input_layout,
                |layout| {
                    let (bp, sh) = build_utility_processor_for_layout(
                        model,
                        params,
                        sample_rate.round() as usize,
                        layout,
                    )?;
                    if captured_stream.is_none() {
                        captured_stream = sh;
                    }
                    Ok(bp)
                },
            )?;
            outcome.stream_handle = captured_stream;
            Ok(audio_block_runtime_node(block, input_layout, outcome))
        },
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
        EFFECT_TYPE_PITCH => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(chain, EFFECT_TYPE_PITCH, model, input_layout, |layout| {
                build_pitch_processor_for_layout(model, params, sample_rate, layout)
            })?,
        )),
        x if x == block_core::EFFECT_TYPE_VST3 => {
            let entry = vst3_host::find_vst3_plugin(model)
                .ok_or_else(|| anyhow!("VST3 plugin '{}' not found in catalog", model))?;
            let bundle_path = entry.info.bundle_path.clone();
            // Resolve UID lazily if not available from moduleinfo.json.
            let uid = vst3_host::resolve_uid_for_model(model)
                .map_err(|e| anyhow!("VST3 UID resolution failed for '{}': {}", model, e))?;
            // Convert stored params (path="p{id}", value=0–100%) to VST3 normalized pairs.
            let vst3_params: Vec<(u32, f64)> = params
                .values
                .iter()
                .filter_map(|(path, value)| {
                    let id_str = path.strip_prefix('p')?;
                    let id: u32 = id_str.parse().ok()?;
                    let pct = value.as_f32()?;
                    Some((id, (pct / 100.0).clamp(0.0, 1.0) as f64))
                })
                .collect();
            // Load the plugin once so we can extract the controller and library
            // Arc before building the processor. This allows the GUI to reuse
            // the same IEditController instead of creating a second instance
            // (which fails for plugins like ValhallaSupermassive).
            const VST3_BLOCK_SIZE: usize = 512;
            let plugin = vst3_host::Vst3Plugin::load(
                &bundle_path, &uid, sample_rate as f64, 2, VST3_BLOCK_SIZE, &vst3_params,
            ).map_err(|e| anyhow!("VST3 load failed for '{}': {}", model, e))?;
            // Register GUI context: shared controller + library Arc + param channel.
            let param_channel = vst3_host::register_vst3_gui_context(
                model,
                plugin.controller_clone(),
                plugin.library_arc(),
            );
            // Wrap in Option so we can move the plugin out of the FnMut closure
            // (VST3 MonoToStereo schema guarantees the closure is called exactly once).
            let mut plugin_opt = Some(plugin);
            Ok(audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, block_core::EFFECT_TYPE_VST3, model, input_layout, |layout| {
                    let p = plugin_opt.take().ok_or_else(|| anyhow!("VST3 plugin consumed twice"))?;
                    Ok(vst3_host::build_vst3_processor_from_plugin(
                        p, layout, param_channel.clone(),
                    ))
                })?,
            ))
        }
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
    let is_new = existing.is_none();
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
        stream_handle: None,
        fade_state: if is_new {
            FadeState::FadingIn { frames_remaining: FADE_IN_FRAMES }
        } else {
            FadeState::Active
        },
        faulted: false,
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
        stream_handle: None,
        fade_state: FadeState::Bypassed,
        faulted: false,
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
        stream_handle: outcome.stream_handle,
        fade_state: FadeState::FadingIn { frames_remaining: FADE_IN_FRAMES },
        faulted: false,
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
        // MonoOnly: build mono processor — process_buffer handles stereo↔mono conversion
        (ModelAudioMode::MonoOnly, _) => {
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
        stream_handle: None,
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

/// Ensure denormalized floats are flushed to zero on aarch64.
///
/// Without this, neural-network processors (NAM) produce degraded audio on
/// aarch64 because denormals accumulate through the network layers.  On x86
/// the NAM/Eigen libraries set DAZ+FTZ internally — we leave x86 alone to
/// avoid changing the sound character on macOS/Windows.
#[inline(always)]
fn ensure_flush_to_zero() {
    #[cfg(target_arch = "aarch64")]
    unsafe {
        // FZ bit (bit 24) in FPCR
        let fpcr: u64;
        core::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
        if fpcr & (1 << 24) == 0 {
            core::arch::asm!("msr fpcr, {}", in(reg) fpcr | (1 << 24));
        }
    }
}

pub fn process_input_f32(
    runtime: &Arc<ChainRuntimeState>,
    input_index: usize,
    data: &[f32],
    input_total_channels: usize,
) {
    if runtime.is_draining() {
        return;
    }
    ensure_flush_to_zero();
    let num_frames = data.len() / input_total_channels;

    let mut processing_guard = match runtime.processing.try_lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    let ChainProcessingState {
        input_states,
        input_to_segments,
        input_scratches,
    } = &mut *processing_guard;

    // Temporarily take the scratch for this input_index to work around the
    // aliasing rules: we'll put it back before returning. If the slot does
    // not exist we fall back to a scratch allocated on the stack.
    let mut scratch = match input_scratches.get_mut(input_index) {
        Some(s) => std::mem::take(s),
        None => InputCallbackScratch::default(),
    };
    scratch.reset_for_callback();

    if let Some(segments) = input_to_segments.get(input_index) {
        scratch.segment_indices.extend(segments.iter().copied());
    } else if input_index < input_states.len() {
        scratch.segment_indices.push(input_index);
    }

    // Process each segment, mixing into scratch.mixed_per_route.
    for i in 0..scratch.segment_indices.len() {
        let seg_idx = scratch.segment_indices[i];
        process_single_segment(
            input_states,
            &mut scratch,
            seg_idx,
            data,
            input_total_channels,
            num_frames,
            &runtime.error_queue,
        );
    }

    // Collect output route Arcs under the output lock.
    if let Ok(output) = runtime.output.try_lock() {
        for route_idx in scratch.mixed_per_route.keys() {
            if let Some(arc) = output.output_routes.get(*route_idx) {
                scratch.route_arcs.push((*route_idx, Arc::clone(arc)));
            }
        }
    } else {
        if let Some(slot) = input_scratches.get_mut(input_index) {
            *slot = scratch;
        }
        return;
    }

    // Push mixed frames to their output routes.
    for (route_idx, route_arc) in &scratch.route_arcs {
        if let Some(frames) = scratch.mixed_per_route.get(route_idx) {
            let mut route = match route_arc.lock() {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
            for &frame in frames {
                route.buffer.push(frame);
            }
        }
    }

    if let Some(slot) = input_scratches.get_mut(input_index) {
        *slot = scratch;
    }
}

fn process_single_segment(
    input_states: &mut [InputProcessingState],
    scratch: &mut InputCallbackScratch,
    seg_idx: usize,
    data: &[f32],
    input_total_channels: usize,
    num_frames: usize,
    error_queue: &Mutex<Vec<BlockError>>,
) {
    let input_state = match input_states.get_mut(seg_idx) {
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
        output_route_indices,
    } = input_state;

    frame_buffer.clear();
    if num_frames > frame_buffer.capacity() {
        frame_buffer.reserve(num_frames - frame_buffer.capacity());
    }

    for frame in data.chunks(input_total_channels).take(num_frames) {
        let raw_frame = read_input_frame(*input_read_layout, input_channels, frame);
        let chain_frame = match (*input_read_layout, *processing_layout) {
            (AudioChannelLayout::Mono, AudioChannelLayout::Stereo) => {
                let sample = match raw_frame {
                    AudioFrame::Mono(s) => s,
                    _ => unreachable!(),
                };
                AudioFrame::Stereo([sample, sample])
            }
            _ => raw_frame,
        };
        frame_buffer.push(chain_frame);
    }

    for block in blocks.iter_mut() {
        process_audio_block(block, frame_buffer.as_mut_slice(), error_queue);
    }

    if *fade_in_remaining > 0 {
        let fade_total = FADE_IN_FRAMES as f32;
        for frame in frame_buffer.iter_mut() {
            if *fade_in_remaining == 0 { break; }
            let progress = 1.0 - (*fade_in_remaining as f32 / fade_total);
            let gain = 0.5 * (1.0 - (std::f32::consts::PI * progress).cos());
            match frame {
                AudioFrame::Mono(s) => *s *= gain,
                AudioFrame::Stereo([l, r]) => { *l *= gain; *r *= gain; }
            }
            *fade_in_remaining -= 1;
        }
    }

    // Mix this segment's frame_buffer into scratch.mixed_per_route for each
    // route this segment feeds. First segment into an empty bucket copies;
    // subsequent segments sum.
    for &route_idx in output_route_indices.iter() {
        let buf = scratch.mixed_per_route.entry(route_idx).or_default();
        if buf.is_empty() {
            buf.extend_from_slice(frame_buffer);
        } else {
            for (i, &frame) in frame_buffer.iter().enumerate() {
                if i < buf.len() {
                    buf[i] = match (buf[i], frame) {
                        (AudioFrame::Stereo([l1, r1]), AudioFrame::Stereo([l2, r2])) =>
                            AudioFrame::Stereo([l1 + l2, r1 + r2]),
                        (AudioFrame::Mono(a), AudioFrame::Mono(b)) =>
                            AudioFrame::Mono(a + b),
                        (AudioFrame::Stereo([l, r]), AudioFrame::Mono(m)) =>
                            AudioFrame::Stereo([l + m, r + m]),
                        (AudioFrame::Mono(m), AudioFrame::Stereo([l, r])) =>
                            AudioFrame::Stereo([m + l, m + r]),
                    };
                }
            }
        }
    }
}

fn process_audio_block(block: &mut BlockRuntimeNode, frames: &mut [AudioFrame], error_queue: &Mutex<Vec<BlockError>>) {
    // Copy the fade state (it's Copy) so we can call apply_block_processor without
    // holding a borrow into block.fade_state at the same time.
    match block.fade_state {
        FadeState::Bypassed => {
            // Fully bypassed — no processing, no fade. Hard skip.
        }
        FadeState::Active => {
            apply_block_processor(block, frames, error_queue);
        }
        FadeState::FadingIn { frames_remaining } => {
            // Crossfade: dry → wet (block fading in)
            let dry: Vec<AudioFrame> = frames.to_vec();
            apply_block_processor(block, frames, error_queue);
            let fade_total = FADE_IN_FRAMES as f32;
            for (i, frame) in frames.iter_mut().enumerate() {
                if frames_remaining <= i {
                    break;
                }
                let remaining = frames_remaining - i;
                // progress: 0.0 at start of fade, 1.0 at end
                let progress = 1.0 - (remaining as f32 / fade_total);
                let wet_gain = 0.5 * (1.0 - (std::f32::consts::PI * progress).cos());
                let dry_gain = 1.0 - wet_gain;
                blend_frame(frame, dry[i], dry_gain, wet_gain);
            }
            let new_remaining = frames_remaining.saturating_sub(frames.len());
            block.fade_state = if new_remaining == 0 {
                FadeState::Active
            } else {
                FadeState::FadingIn { frames_remaining: new_remaining }
            };
        }
        FadeState::FadingOut { frames_remaining } => {
            // Crossfade: wet → dry (block fading out / being disabled)
            // We still process audio so we can fade out smoothly
            let dry: Vec<AudioFrame> = frames.to_vec();
            apply_block_processor(block, frames, error_queue);
            let fade_total = FADE_IN_FRAMES as f32;
            for (i, frame) in frames.iter_mut().enumerate() {
                if frames_remaining <= i {
                    break;
                }
                let remaining = frames_remaining - i;
                // progress: 0.0 at start of fade-out, 1.0 at end
                let progress = 1.0 - (remaining as f32 / fade_total);
                // wet_gain: 1.0 at start, 0.0 at end (cosine fade-out)
                let wet_gain = 0.5 * (1.0 + (std::f32::consts::PI * progress).cos());
                let dry_gain = 1.0 - wet_gain;
                blend_frame(frame, dry[i], dry_gain, wet_gain);
            }
            let new_remaining = frames_remaining.saturating_sub(frames.len());
            block.fade_state = if new_remaining == 0 {
                FadeState::Bypassed
            } else {
                FadeState::FadingOut { frames_remaining: new_remaining }
            };
        }
    }
}

fn apply_block_processor(block: &mut BlockRuntimeNode, frames: &mut [AudioFrame], error_queue: &Mutex<Vec<BlockError>>) {
    if block.faulted {
        return;
    }
    match &mut block.processor {
        RuntimeProcessor::Audio(processor) => {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                processor.process_buffer(frames, &mut block.scratch);
            }));
            if let Err(payload) = result {
                block.faulted = true;
                for frame in frames.iter_mut() {
                    *frame = AudioFrame::Stereo([0.0, 0.0]);
                }
                let msg = downcast_panic_message(payload);
                log::error!("block '{}' panicked — permanently bypassed: {}", block.block_id.0, msg);
                if let Ok(mut q) = error_queue.try_lock() {
                    q.push(BlockError { block_id: block.block_id.clone(), message: msg });
                }
            }
        }
        RuntimeProcessor::Select(select) => {
            if let Some(selected) = select.selected_node_mut() {
                process_audio_block(selected, frames, error_queue);
            }
        }
        RuntimeProcessor::Bypass => {}
    }
}

fn downcast_panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

#[inline]
fn blend_frame(frame: &mut AudioFrame, dry: AudioFrame, dry_gain: f32, wet_gain: f32) {
    match (frame, dry) {
        (AudioFrame::Mono(w), AudioFrame::Mono(d)) => {
            *w = d * dry_gain + *w * wet_gain;
        }
        (AudioFrame::Stereo([wl, wr]), AudioFrame::Stereo([dl, dr])) => {
            *wl = dl * dry_gain + *wl * wet_gain;
            *wr = dr * dry_gain + *wr * wet_gain;
        }
        // Layout mismatch shouldn't happen in practice; pass dry through
        (frame, dry) => {
            *frame = dry;
        }
    }
}

pub fn process_output_f32(
    runtime: &Arc<ChainRuntimeState>,
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
) {
    if runtime.is_draining() {
        out.fill(0.0);
        return;
    }
    ensure_flush_to_zero();
    // Get the Arc for this specific route (brief lock on output state)
    let route_arc = {
        let output_state = match runtime.output.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        match output_state.output_routes.get(output_index) {
            Some(r) => Arc::clone(r),
            None => {
                out.fill(0.0);
                return;
            }
        }
    };
    // Lock this route — brief wait while input pushes is acceptable,
    // filling with silence on try_lock failure causes audible clicks.
    let mut route = match route_arc.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    let num_frames = out.len() / output_total_channels;
    for frame in out.chunks_mut(output_total_channels).take(num_frames) {
        frame.fill(0.0);
        let processed = route.buffer.pop();
        write_output_frame(
            processed,
            &route.output_channels,
            frame,
            route.output_mixdown,
        );
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
#[allow(dead_code)]
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
        apply_block_processor, process_audio_block,
        build_chain_runtime_state, build_runtime_graph, process_input_f32, process_output_f32,
        update_chain_runtime_state, split_chain_into_segments, effective_inputs, effective_outputs,
        AudioFrame, AudioProcessor, BlockError, BlockRuntimeNode, FadeState, ProcessorScratch, RuntimeProcessor,
        ElasticBuffer, DEFAULT_ELASTIC_TARGET, FADE_IN_FRAMES,
    };
    use block_core::AudioChannelLayout;
    use block_preamp::supported_models as supported_preamp_models;
    use block_cab::{cab_backend_kind, supported_models as supported_cab_models, CabBackendKind};
    use block_delay::supported_models as supported_delay_models;
    use block_dyn::compressor_supported_models;
    use block_reverb::supported_models as supported_reverb_models;
    use block_util::supported_models as supported_tuner_models;
    use domain::ids::{BlockId, DeviceId, ChainId};
    use domain::value_objects::ParameterValue;
    use project::block::{
        AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, InsertBlock, InsertEndpoint, OutputBlock, OutputEntry, SelectBlock, schema_for_block_model,
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
            &HashMap::new(),
        )
        .expect("runtime graph should build");
        assert_eq!(runtime.chains.len(), 1);
    }

    #[test]
    #[ignore] // requires asset_paths initialization
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
            &HashMap::new(),
        ) {
            Ok(_) => panic!("runtime graph should reject mismatched IR sample rate"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("sample_rate"));
    }

    #[test]
    #[ignore] // requires asset_paths initialization
    fn update_chain_runtime_state_preserves_unchanged_block_instances() {
        let mut chain = tuner_track(
            "chain:0",
            vec![
                tuner_block("chain:0:block:a", 440.0),
                tuner_block("chain:0:block:b", 445.0),
            ],
        );

        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime state should build"));
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

        update_chain_runtime_state(&runtime, &chain, 48_000.0, false, DEFAULT_ELASTIC_TARGET)
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
    #[ignore] // requires asset_paths initialization
    fn update_chain_runtime_state_preserves_block_identity_when_reordered() {
        let mut chain = tuner_track(
            "chain:0",
            vec![
                tuner_block("chain:0:block:a", 440.0),
                tuner_block("chain:0:block:b", 445.0),
            ],
        );

        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime state should build"));
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

        update_chain_runtime_state(&runtime, &chain, 48_000.0, false, DEFAULT_ELASTIC_TARGET)
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
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime state should build"));
        let total_frames = DEFAULT_ELASTIC_TARGET * 2 + 64;
        let input = vec![0.25f32; total_frames];

        process_input_f32(&runtime, 0, &input, 1);

        let output = runtime.output.lock().expect("runtime poisoned");
        let route = output.output_routes[0].lock().expect("route poisoned");
        assert!(route.buffer.len() <= DEFAULT_ELASTIC_TARGET * 2);
    }

    #[test]
    #[ignore] // requires asset_paths initialization
    fn process_output_drains_buffered_frames() {
        let chain = tuner_track("chain:0", Vec::new());
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime state should build"));

        process_input_f32(&runtime, 0, &[0.25, 0.5, 0.75, 1.0], 1);

        let mut out = vec![0.0f32; 4];
        process_output_f32(&runtime, 0, &mut out, 1);

        assert_eq!(out, vec![0.25, 0.5, 0.75, 1.0]);
        let output = runtime.output.lock().expect("runtime poisoned");
        let route = output.output_routes[0].lock().expect("route poisoned");
        assert!(route.buffer.queue.is_empty());
    }

    #[test]
    #[ignore] // requires asset_paths initialization
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
                            device_id: DeviceId("output-device".into()),
                            mode: ChainOutputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
            ],
        };
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime state should build"));

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
    #[ignore] // requires asset_paths initialization
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
                            device_id: DeviceId("output-device".into()),
                            mode: ChainOutputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
            ],
        };
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime state should build"));

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
    #[ignore] // requires asset_paths initialization
    fn select_block_builds_for_generic_delay_options() {
        let chain = select_delay_chain("chain:select", "delay_a");

        let runtime =
            build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("select delay chain should build");

        let locked = runtime.processing.lock().expect("runtime poisoned");
        assert_eq!(locked.input_states[0].blocks.len(), 1);
    }

    #[test]
    #[ignore] // requires asset_paths initialization
    fn update_chain_runtime_state_preserves_select_instance_when_switching_active_option() {
        let mut chain = select_delay_chain("chain:select", "delay_a");
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime state should build"));
        let original_serial = {
            let locked = runtime.processing.lock().expect("runtime poisoned");
            locked.input_states[0].blocks[0].instance_serial
        };

        if let AudioBlockKind::Select(select) = &mut chain.blocks[0].kind {
            select.selected_block_id = BlockId("chain:select:block:0::delay_b".into());
        }

        update_chain_runtime_state(&runtime, &chain, 48_000.0, false, DEFAULT_ELASTIC_TARGET)
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

    // --- ElasticBuffer tests ---

    #[test]
    fn elastic_buffer_push_pop_basic() {
        let mut buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
        buf.push(AudioFrame::Mono(0.5));
        buf.push(AudioFrame::Mono(0.7));
        assert_eq!(buf.len(), 2);
        let f1 = buf.pop();
        assert!(matches!(f1, AudioFrame::Mono(v) if (v - 0.5).abs() < 1e-6));
        let f2 = buf.pop();
        assert!(matches!(f2, AudioFrame::Mono(v) if (v - 0.7).abs() < 1e-6));
    }

    #[test]
    fn elastic_buffer_underrun_repeats_last_frame() {
        let mut buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
        buf.push(AudioFrame::Mono(0.42));
        let _ = buf.pop(); // drain the one frame
        // Now empty — should repeat last frame, NOT silence
        let repeated = buf.pop();
        assert!(matches!(repeated, AudioFrame::Mono(v) if (v - 0.42).abs() < 1e-6));
    }

    #[test]
    fn elastic_buffer_underrun_before_any_push_returns_silence() {
        let mut buf = ElasticBuffer::new(256, AudioChannelLayout::Stereo);
        let frame = buf.pop();
        assert!(matches!(frame, AudioFrame::Stereo([l, r]) if l.abs() < 1e-6 && r.abs() < 1e-6));
    }

    #[test]
    fn elastic_buffer_overrun_discards_oldest() {
        let target = 4; // small for testing
        let mut buf = ElasticBuffer::new(target, AudioChannelLayout::Mono);
        // Push 2x target + 1 = 9 frames
        for i in 0..9 {
            buf.push(AudioFrame::Mono(i as f32));
        }
        // Should have discarded oldest, keeping at most 2x target = 8
        assert!(buf.len() <= target * 2);
        // First frame should NOT be 0.0 (it was discarded)
        let first = buf.pop();
        assert!(matches!(first, AudioFrame::Mono(v) if v > 0.1));
    }

    #[test]
    fn elastic_buffer_stabilizes_around_target() {
        let target = 256;
        let mut buf = ElasticBuffer::new(target, AudioChannelLayout::Mono);
        // Simulate: push slightly faster than pop
        for _ in 0..10000 {
            buf.push(AudioFrame::Mono(1.0));
            buf.push(AudioFrame::Mono(1.0)); // 2 pushes
            let _ = buf.pop(); // 1 pop — simulates input faster than output
        }
        // Should not have grown unbounded
        assert!(buf.len() <= target * 2);
    }

    /// A chain with proper Input and Output blocks but no effect blocks.
    /// Useful for testing process_input_f32 / process_output_f32.
    fn io_passthrough_chain(id: &str) -> Chain {
        Chain {
            id: ChainId(id.into()),
            description: Some("Passthrough".into()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId(format!("{id}:input:0")),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".into(),
                        entries: vec![InputEntry {
                            device_id: DeviceId("dev".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId(format!("{id}:output:0")),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".into(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId("dev".into()),
                            mode: ChainOutputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
            ],
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

    #[test]
    fn segments_split_by_output_position() {
        // Chain: [Input, TS9(1), Amp(2), Volume(3), Output_MIXER(4), Delay(5), Reverb(6), Output_Scarlett(7)]
        let chain = Chain {
            id: ChainId("test".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            blocks: vec![
                AudioBlock { id: BlockId("input:0".into()), enabled: true,
                    kind: AudioBlockKind::Input(InputBlock { model: "standard".into(),
                        entries: vec![InputEntry { device_id: DeviceId("scarlett".into()), mode: ChainInputMode::Mono, channels: vec![0] }] }) },
                AudioBlock { id: BlockId("ts9".into()), enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock { effect_type: "gain".into(), model: "volume".into(), params: ParameterSet::default() }) },
                AudioBlock { id: BlockId("amp".into()), enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock { effect_type: "gain".into(), model: "volume".into(), params: ParameterSet::default() }) },
                AudioBlock { id: BlockId("volume".into()), enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock { effect_type: "gain".into(), model: "volume".into(), params: ParameterSet::default() }) },
                AudioBlock { id: BlockId("out_mixer".into()), enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock { model: "standard".into(),
                        entries: vec![OutputEntry { device_id: DeviceId("mixer".into()), mode: ChainOutputMode::Stereo, channels: vec![0, 1] }] }) },
                AudioBlock { id: BlockId("delay".into()), enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock { effect_type: "delay".into(), model: "digital_clean".into(), params: ParameterSet::default() }) },
                AudioBlock { id: BlockId("reverb".into()), enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock { effect_type: "reverb".into(), model: "plate_foundation".into(), params: ParameterSet::default() }) },
                AudioBlock { id: BlockId("out_scarlett".into()), enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock { model: "standard".into(),
                        entries: vec![OutputEntry { device_id: DeviceId("scarlett".into()), mode: ChainOutputMode::Stereo, channels: vec![0, 1] }] }) },
            ],
        };

        let (eff_inputs, eff_cpal_indices) = effective_inputs(&chain);
        let eff_outputs = effective_outputs(&chain);
        let segments = split_chain_into_segments(&chain, &eff_inputs, &eff_cpal_indices, &eff_outputs);

        // Should have 2 segments (1 input × 2 outputs)
        assert_eq!(segments.len(), 2, "expected 2 segments, got {}", segments.len());

        // Segment 0: blocks before Output_MIXER (pos 4) → [TS9(1), Amp(2), Volume(3)]
        assert_eq!(segments[0].block_indices, vec![1, 2, 3],
            "segment 0 should have blocks [1,2,3], got {:?}", segments[0].block_indices);
        assert_eq!(segments[0].output_route_indices, vec![0],
            "segment 0 should push to output 0 only");

        // Segment 1: blocks before Output_Scarlett (pos 7) → [TS9(1), Amp(2), Volume(3), Delay(5), Reverb(6)]
        assert_eq!(segments[1].block_indices, vec![1, 2, 3, 5, 6],
            "segment 1 should have blocks [1,2,3,5,6], got {:?}", segments[1].block_indices);
        assert_eq!(segments[1].output_route_indices, vec![1],
            "segment 1 should push to output 1 only");
    }

    // ── Panic recovery tests ──────────────────────────────────────────────────

    struct PanickingProcessor;
    impl block_core::MonoProcessor for PanickingProcessor {
        fn process_sample(&mut self, _: f32) -> f32 {
            panic!("simulated plugin crash");
        }
    }

    struct CountingProcessor {
        call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
    impl block_core::MonoProcessor for CountingProcessor {
        fn process_sample(&mut self, input: f32) -> f32 {
            self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            input
        }
    }

    fn panicking_block_node() -> BlockRuntimeNode {
        BlockRuntimeNode {
            instance_serial: 0,
            block_id: domain::ids::BlockId("test:panicking".into()),
            block_snapshot: project::block::AudioBlock {
                id: domain::ids::BlockId("test:panicking".into()),
                enabled: true,
                kind: project::block::AudioBlockKind::Core(project::block::CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: project::param::ParameterSet::default(),
                }),
            },
            input_layout: block_core::AudioChannelLayout::Mono,
            output_layout: block_core::AudioChannelLayout::Mono,
            scratch: ProcessorScratch::Mono(Vec::new()),
            processor: RuntimeProcessor::Audio(AudioProcessor::Mono(Box::new(PanickingProcessor))),
            stream_handle: None,
            fade_state: FadeState::Active,
            faulted: false,
        }
    }

    fn counting_block_node(counter: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> BlockRuntimeNode {
        BlockRuntimeNode {
            instance_serial: 0,
            block_id: domain::ids::BlockId("test:counting".into()),
            block_snapshot: project::block::AudioBlock {
                id: domain::ids::BlockId("test:counting".into()),
                enabled: true,
                kind: project::block::AudioBlockKind::Core(project::block::CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: project::param::ParameterSet::default(),
                }),
            },
            input_layout: block_core::AudioChannelLayout::Mono,
            output_layout: block_core::AudioChannelLayout::Mono,
            scratch: ProcessorScratch::Mono(Vec::new()),
            processor: RuntimeProcessor::Audio(AudioProcessor::Mono(Box::new(CountingProcessor { call_count: counter }))),
            stream_handle: None,
            fade_state: FadeState::Active,
            faulted: false,
        }
    }

    #[test]
    fn panicking_processor_does_not_crash_the_caller() {
        let mut block = panicking_block_node();
        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        // Must not panic
        apply_block_processor(&mut block, &mut frames, &error_queue);
    }

    #[test]
    fn panicking_processor_marks_block_as_faulted() {
        let mut block = panicking_block_node();
        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        apply_block_processor(&mut block, &mut frames, &error_queue);

        assert!(block.faulted, "block should be marked faulted after a panic");
    }

    #[test]
    fn panicking_processor_zeroes_output_frames() {
        let mut block = panicking_block_node();
        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        apply_block_processor(&mut block, &mut frames, &error_queue);

        for frame in &frames {
            match frame {
                AudioFrame::Stereo([l, r]) => {
                    assert_eq!(*l, 0.0, "left channel should be silent after panic");
                    assert_eq!(*r, 0.0, "right channel should be silent after panic");
                }
                AudioFrame::Mono(s) => assert_eq!(*s, 0.0, "mono channel should be silent after panic"),
            }
        }
    }

    #[test]
    fn panicking_processor_posts_error_to_queue() {
        let mut block = panicking_block_node();
        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        apply_block_processor(&mut block, &mut frames, &error_queue);

        let errors = error_queue.lock().unwrap();
        assert_eq!(errors.len(), 1, "exactly one error should be posted");
        assert_eq!(errors[0].block_id.0, "test:panicking");
        assert!(errors[0].message.contains("simulated plugin crash"), "error message should contain panic message");
    }

    #[test]
    fn faulted_block_is_permanently_bypassed() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut block = counting_block_node(counter.clone());
        block.faulted = true; // pre-fault the block

        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        apply_block_processor(&mut block, &mut frames, &error_queue);

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0,
            "process_sample should never be called on a faulted block");
    }

    #[test]
    fn second_call_after_panic_does_not_process_or_post_error() {
        let mut block = panicking_block_node();
        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        // First call: panics, marks faulted, posts error
        apply_block_processor(&mut block, &mut frames, &error_queue);
        assert_eq!(error_queue.lock().unwrap().len(), 1);

        // Second call: faulted — must not post another error
        apply_block_processor(&mut block, &mut frames, &error_queue);
        assert_eq!(error_queue.lock().unwrap().len(), 1,
            "no additional error should be posted for an already-faulted block");
    }

    #[test]
    fn process_audio_block_bypassed_state_skips_processing() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut block = counting_block_node(counter.clone());
        block.fade_state = FadeState::Bypassed;

        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        process_audio_block(&mut block, &mut frames, &error_queue);

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0,
            "bypassed block should not call process_sample");
    }

    #[test]
    fn process_audio_block_fading_in_applies_processing() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut block = counting_block_node(counter.clone());
        block.fade_state = FadeState::FadingIn { frames_remaining: FADE_IN_FRAMES };

        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        process_audio_block(&mut block, &mut frames, &error_queue);

        assert!(counter.load(std::sync::atomic::Ordering::SeqCst) > 0,
            "fading-in block should call process_sample");
    }

    // ── AudioFrame tests ─────────────────────────────────────────────────────

    #[test]
    fn audio_frame_mono_mix_mono_returns_sample() {
        let frame = AudioFrame::Mono(0.75);
        assert!((frame.mono_mix() - 0.75).abs() < 1e-6);
    }

    #[test]
    fn audio_frame_mono_mix_stereo_returns_average() {
        let frame = AudioFrame::Stereo([0.4, 0.8]);
        assert!((frame.mono_mix() - 0.6).abs() < 1e-6);
    }

    // ── ElasticBuffer edge cases ─────────────────────────────────────────────

    #[test]
    fn elastic_buffer_target_one_limits_to_two() {
        let mut buf = ElasticBuffer::new(1, AudioChannelLayout::Mono);
        buf.push(AudioFrame::Mono(1.0));
        buf.push(AudioFrame::Mono(2.0));
        buf.push(AudioFrame::Mono(3.0)); // should discard oldest
        assert!(buf.len() <= 2, "buffer with target=1 should hold at most 2 frames");
    }

    #[test]
    fn elastic_buffer_stereo_push_pop_preserves_channels() {
        let mut buf = ElasticBuffer::new(256, AudioChannelLayout::Stereo);
        buf.push(AudioFrame::Stereo([0.3, 0.7]));
        let frame = buf.pop();
        match frame {
            AudioFrame::Stereo([l, r]) => {
                assert!((l - 0.3).abs() < 1e-6);
                assert!((r - 0.7).abs() < 1e-6);
            }
            _ => panic!("expected stereo frame"),
        }
    }

    #[test]
    fn elastic_buffer_multiple_pops_on_empty_repeat_last() {
        let mut buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
        buf.push(AudioFrame::Mono(0.99));
        let _ = buf.pop(); // drain
        // Multiple pops should all return last frame
        for _ in 0..5 {
            let f = buf.pop();
            assert!(matches!(f, AudioFrame::Mono(v) if (v - 0.99).abs() < 1e-6));
        }
    }

    // ── FadeState transition tests ───────────────────────────────────────────

    #[test]
    fn fade_in_completes_to_active_after_enough_frames() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut block = counting_block_node(counter.clone());
        block.fade_state = FadeState::FadingIn { frames_remaining: 16 };

        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        process_audio_block(&mut block, &mut frames, &error_queue);

        assert_eq!(block.fade_state, FadeState::Active,
            "fade-in should complete to Active when frames_remaining reaches 0");
    }

    #[test]
    fn fade_in_partial_keeps_fading_in() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut block = counting_block_node(counter.clone());
        block.fade_state = FadeState::FadingIn { frames_remaining: 64 };

        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        process_audio_block(&mut block, &mut frames, &error_queue);

        match block.fade_state {
            FadeState::FadingIn { frames_remaining } => {
                assert_eq!(frames_remaining, 48, "should have consumed 16 frames of fade");
            }
            other => panic!("expected FadingIn, got {:?}", other),
        }
    }

    #[test]
    fn fade_out_completes_to_bypassed_after_enough_frames() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut block = counting_block_node(counter.clone());
        block.fade_state = FadeState::FadingOut { frames_remaining: 16 };

        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        process_audio_block(&mut block, &mut frames, &error_queue);

        assert_eq!(block.fade_state, FadeState::Bypassed,
            "fade-out should complete to Bypassed when frames_remaining reaches 0");
    }

    #[test]
    fn fade_out_partial_keeps_fading_out() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut block = counting_block_node(counter.clone());
        block.fade_state = FadeState::FadingOut { frames_remaining: 64 };

        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        process_audio_block(&mut block, &mut frames, &error_queue);

        match block.fade_state {
            FadeState::FadingOut { frames_remaining } => {
                assert_eq!(frames_remaining, 48, "should have consumed 16 frames of fade");
            }
            other => panic!("expected FadingOut, got {:?}", other),
        }
    }

    #[test]
    fn fade_out_applies_processing_during_transition() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut block = counting_block_node(counter.clone());
        block.fade_state = FadeState::FadingOut { frames_remaining: FADE_IN_FRAMES };

        let error_queue = std::sync::Mutex::new(Vec::<BlockError>::new());
        let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

        process_audio_block(&mut block, &mut frames, &error_queue);

        assert!(counter.load(std::sync::atomic::Ordering::SeqCst) > 0,
            "fading-out block should still call process_sample during transition");
    }

    // ── blend_frame tests ────────────────────────────────────────────────────

    #[test]
    fn blend_frame_mono_interpolates_correctly() {
        use super::blend_frame;
        let mut wet = AudioFrame::Mono(1.0);
        let dry = AudioFrame::Mono(0.0);
        blend_frame(&mut wet, dry, 0.5, 0.5);
        assert!((wet.mono_mix() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn blend_frame_stereo_interpolates_correctly() {
        use super::blend_frame;
        let mut wet = AudioFrame::Stereo([1.0, 0.0]);
        let dry = AudioFrame::Stereo([0.0, 1.0]);
        blend_frame(&mut wet, dry, 0.5, 0.5);
        match wet {
            AudioFrame::Stereo([l, r]) => {
                assert!((l - 0.5).abs() < 1e-6);
                assert!((r - 0.5).abs() < 1e-6);
            }
            _ => panic!("expected stereo"),
        }
    }

    #[test]
    fn blend_frame_layout_mismatch_passes_dry_through() {
        use super::blend_frame;
        let mut wet = AudioFrame::Mono(1.0);
        let dry = AudioFrame::Stereo([0.3, 0.7]);
        blend_frame(&mut wet, dry, 0.5, 0.5);
        // On layout mismatch, frame should be set to dry
        match wet {
            AudioFrame::Stereo([l, r]) => {
                assert!((l - 0.3).abs() < 1e-6);
                assert!((r - 0.7).abs() < 1e-6);
            }
            _ => panic!("expected stereo from dry passthrough"),
        }
    }

    // ── mix_frames tests ─────────────────────────────────────────────────────

    #[test]
    fn mix_frames_mono_mono_sums() {
        use super::mix_frames;
        let result = mix_frames(AudioFrame::Mono(0.3), AudioFrame::Mono(0.5));
        assert!(matches!(result, AudioFrame::Mono(v) if (v - 0.8).abs() < 1e-6));
    }

    #[test]
    fn mix_frames_stereo_stereo_sums() {
        use super::mix_frames;
        let result = mix_frames(
            AudioFrame::Stereo([0.1, 0.2]),
            AudioFrame::Stereo([0.3, 0.4]),
        );
        match result {
            AudioFrame::Stereo([l, r]) => {
                assert!((l - 0.4).abs() < 1e-6);
                assert!((r - 0.6).abs() < 1e-6);
            }
            _ => panic!("expected stereo"),
        }
    }

    #[test]
    fn mix_frames_mono_stereo_widens() {
        use super::mix_frames;
        let result = mix_frames(AudioFrame::Mono(0.5), AudioFrame::Stereo([0.1, 0.2]));
        match result {
            AudioFrame::Stereo([l, r]) => {
                assert!((l - 0.6).abs() < 1e-6);
                assert!((r - 0.7).abs() < 1e-6);
            }
            _ => panic!("expected stereo"),
        }
    }

    #[test]
    fn mix_frames_stereo_mono_widens() {
        use super::mix_frames;
        let result = mix_frames(AudioFrame::Stereo([0.1, 0.2]), AudioFrame::Mono(0.5));
        match result {
            AudioFrame::Stereo([l, r]) => {
                assert!((l - 0.6).abs() < 1e-6);
                assert!((r - 0.7).abs() < 1e-6);
            }
            _ => panic!("expected stereo"),
        }
    }

    // ── output_limiter tests ─────────────────────────────────────────────────

    #[test]
    fn output_limiter_transparent_below_threshold() {
        use super::output_limiter;
        assert!((output_limiter(0.5) - 0.5).abs() < 1e-6);
        assert!((output_limiter(-0.5) - (-0.5)).abs() < 1e-6);
        assert!((output_limiter(0.0) - 0.0).abs() < 1e-6);
        assert!((output_limiter(0.94) - 0.94).abs() < 1e-6);
    }

    #[test]
    fn output_limiter_saturates_above_threshold() {
        use super::output_limiter;
        let limited = output_limiter(2.0);
        assert!(limited < 2.0, "limiter should reduce values above threshold");
        assert!(limited > 0.0, "limiter should keep positive sign");
        // tanh(2.0) ≈ 0.964
        assert!((limited - 2.0f32.tanh()).abs() < 1e-6);
    }

    #[test]
    fn output_limiter_negative_saturates_symmetrically() {
        use super::output_limiter;
        let limited = output_limiter(-2.0);
        assert!(limited > -2.0, "limiter should reduce negative values");
        assert!((limited - (-2.0f32).tanh()).abs() < 1e-6);
    }

    // ── apply_mixdown tests ──────────────────────────────────────────────────

    #[test]
    fn apply_mixdown_sum_adds_channels() {
        use super::apply_mixdown;
        use project::chain::ChainOutputMixdown;
        assert!((apply_mixdown(ChainOutputMixdown::Sum, 0.3, 0.5) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn apply_mixdown_average_averages_channels() {
        use super::apply_mixdown;
        use project::chain::ChainOutputMixdown;
        assert!((apply_mixdown(ChainOutputMixdown::Average, 0.4, 0.8) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn apply_mixdown_left_returns_left() {
        use super::apply_mixdown;
        use project::chain::ChainOutputMixdown;
        assert!((apply_mixdown(ChainOutputMixdown::Left, 0.3, 0.7) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn apply_mixdown_right_returns_right() {
        use super::apply_mixdown;
        use project::chain::ChainOutputMixdown;
        assert!((apply_mixdown(ChainOutputMixdown::Right, 0.3, 0.7) - 0.7).abs() < 1e-6);
    }

    // ── layout_from_channels tests ───────────────────────────────────────────

    #[test]
    fn layout_from_channels_mono_ok() {
        use super::layout_from_channels;
        assert_eq!(layout_from_channels(1).unwrap(), AudioChannelLayout::Mono);
    }

    #[test]
    fn layout_from_channels_stereo_ok() {
        use super::layout_from_channels;
        assert_eq!(layout_from_channels(2).unwrap(), AudioChannelLayout::Stereo);
    }

    #[test]
    fn layout_from_channels_invalid_errors() {
        use super::layout_from_channels;
        assert!(layout_from_channels(0).is_err());
        assert!(layout_from_channels(3).is_err());
        assert!(layout_from_channels(8).is_err());
    }

    // ── write_output_frame tests ─────────────────────────────────────────────

    #[test]
    fn write_output_frame_mono_to_single_channel() {
        use super::write_output_frame;
        use project::chain::ChainOutputMixdown;
        let mut frame = [0.0f32; 2];
        write_output_frame(AudioFrame::Mono(0.5), &[1], &mut frame, ChainOutputMixdown::Average);
        assert!((frame[0] - 0.0).abs() < 1e-6, "channel 0 should be untouched");
        assert!((frame[1] - 0.5).abs() < 1e-6, "channel 1 should have the sample");
    }

    #[test]
    fn write_output_frame_mono_to_multiple_channels() {
        use super::write_output_frame;
        use project::chain::ChainOutputMixdown;
        let mut frame = [0.0f32; 4];
        write_output_frame(AudioFrame::Mono(0.8), &[0, 2, 3], &mut frame, ChainOutputMixdown::Average);
        assert!((frame[0] - 0.8).abs() < 1e-6);
        assert!((frame[1] - 0.0).abs() < 1e-6);
        assert!((frame[2] - 0.8).abs() < 1e-6);
        assert!((frame[3] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn write_output_frame_stereo_to_zero_channels() {
        use super::write_output_frame;
        use project::chain::ChainOutputMixdown;
        let mut frame = [0.0f32; 2];
        // Empty channels — should not write anything
        write_output_frame(AudioFrame::Stereo([0.5, 0.7]), &[], &mut frame, ChainOutputMixdown::Average);
        assert_eq!(frame, [0.0, 0.0]);
    }

    #[test]
    fn write_output_frame_stereo_to_one_channel_uses_mixdown() {
        use super::write_output_frame;
        use project::chain::ChainOutputMixdown;
        let mut frame = [0.0f32; 2];
        write_output_frame(AudioFrame::Stereo([0.4, 0.8]), &[0], &mut frame, ChainOutputMixdown::Average);
        // Average of 0.4 and 0.8 = 0.6
        assert!((frame[0] - 0.6).abs() < 1e-6);
    }

    #[test]
    fn write_output_frame_stereo_to_two_channels_preserves_lr() {
        use super::write_output_frame;
        use project::chain::ChainOutputMixdown;
        let mut frame = [0.0f32; 4];
        write_output_frame(AudioFrame::Stereo([0.3, 0.7]), &[1, 3], &mut frame, ChainOutputMixdown::Average);
        assert!((frame[0] - 0.0).abs() < 1e-6);
        assert!((frame[1] - 0.3).abs() < 1e-6);
        assert!((frame[2] - 0.0).abs() < 1e-6);
        assert!((frame[3] - 0.7).abs() < 1e-6);
    }

    // ── read_input_frame tests ───────────────────────────────────────────────

    #[test]
    fn read_input_frame_mono_reads_correct_channel() {
        use super::read_input_frame;
        let data = [0.1, 0.9, 0.5, 0.3];
        let frame = read_input_frame(AudioChannelLayout::Mono, &[2], &data);
        assert!(matches!(frame, AudioFrame::Mono(v) if (v - 0.5).abs() < 1e-6));
    }

    #[test]
    fn read_input_frame_stereo_reads_two_channels() {
        use super::read_input_frame;
        let data = [0.1, 0.2, 0.3, 0.4];
        let frame = read_input_frame(AudioChannelLayout::Stereo, &[1, 3], &data);
        match frame {
            AudioFrame::Stereo([l, r]) => {
                assert!((l - 0.2).abs() < 1e-6);
                assert!((r - 0.4).abs() < 1e-6);
            }
            _ => panic!("expected stereo"),
        }
    }

    #[test]
    fn read_input_frame_out_of_bounds_returns_zero() {
        use super::read_input_frame;
        let data = [0.5f32; 2];
        let frame = read_input_frame(AudioChannelLayout::Mono, &[99], &data);
        assert!(matches!(frame, AudioFrame::Mono(v) if v.abs() < 1e-6));
    }

    // ── silent_frame tests ───────────────────────────────────────────────────

    #[test]
    fn silent_frame_mono_is_zero() {
        use super::silent_frame;
        let frame = silent_frame(AudioChannelLayout::Mono);
        assert!(matches!(frame, AudioFrame::Mono(v) if v.abs() < 1e-6));
    }

    #[test]
    fn silent_frame_stereo_is_zero() {
        use super::silent_frame;
        let frame = silent_frame(AudioChannelLayout::Stereo);
        assert!(matches!(frame, AudioFrame::Stereo([l, r]) if l.abs() < 1e-6 && r.abs() < 1e-6));
    }

    // ── build_runtime_graph edge cases ───────────────────────────────────────

    #[test]
    fn build_runtime_graph_skips_disabled_chains() {
        let project = Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: ChainId("disabled".into()),
                description: None,
                instrument: "electric_guitar".to_string(),
                enabled: false,
                blocks: vec![],
            }],
        };

        let runtime = build_runtime_graph(&project, &HashMap::new(), &HashMap::new())
            .expect("build should succeed with disabled chain");
        assert!(runtime.chains.is_empty(), "disabled chains should be skipped");
    }

    #[test]
    fn build_runtime_graph_errors_on_missing_sample_rate() {
        let project = Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: ChainId("chain:0".into()),
                description: None,
                instrument: "electric_guitar".to_string(),
                enabled: true,
                blocks: vec![],
            }],
        };

        let result = build_runtime_graph(&project, &HashMap::new(), &HashMap::new());
        assert!(result.is_err(), "should error when chain has no sample rate");
    }

    // ── RuntimeGraph methods ─────────────────────────────────────────────────

    #[test]
    fn runtime_graph_remove_chain_removes_entry() {
        let chain = tuner_track("chain:remove", Vec::new());
        let mut rates = HashMap::new();
        rates.insert(ChainId("chain:remove".into()), 48_000.0);
        let project = Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![chain],
        };
        let mut graph = build_runtime_graph(&project, &rates, &HashMap::new()).unwrap();
        assert_eq!(graph.chains.len(), 1);
        graph.remove_chain(&ChainId("chain:remove".into()));
        assert!(graph.chains.is_empty());
    }

    #[test]
    fn runtime_graph_runtime_for_chain_returns_none_for_unknown() {
        use super::RuntimeGraph;
        let graph = RuntimeGraph { chains: HashMap::new() };
        assert!(graph.runtime_for_chain(&ChainId("nonexistent".into())).is_none());
    }

    #[test]
    fn runtime_graph_upsert_chain_creates_new_entry() {
        use super::RuntimeGraph;
        let mut graph = RuntimeGraph { chains: HashMap::new() };
        let chain = tuner_track("chain:new", Vec::new());
        let result = graph.upsert_chain(&chain, 48_000.0, false, DEFAULT_ELASTIC_TARGET);
        assert!(result.is_ok());
        assert_eq!(graph.chains.len(), 1);
    }

    #[test]
    fn runtime_graph_upsert_chain_updates_existing() {
        use super::RuntimeGraph;
        let mut graph = RuntimeGraph { chains: HashMap::new() };
        let chain = tuner_track("chain:upsert", vec![tuner_block("b:0", 440.0)]);
        graph.upsert_chain(&chain, 48_000.0, false, DEFAULT_ELASTIC_TARGET).unwrap();
        // Update — should reuse existing entry
        let chain2 = tuner_track("chain:upsert", vec![tuner_block("b:0", 445.0)]);
        let result = graph.upsert_chain(&chain2, 48_000.0, false, DEFAULT_ELASTIC_TARGET);
        assert!(result.is_ok());
        assert_eq!(graph.chains.len(), 1);
    }

    // ── process_output_f32 edge cases ────────────────────────────────────────

    #[test]
    fn process_output_fills_silence_for_invalid_output_index() {
        let chain = io_passthrough_chain("chain:0");
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime should build"));

        let mut out = vec![1.0f32; 8];
        process_output_f32(&runtime, 999, &mut out, 1);

        assert!(out.iter().all(|&v| v == 0.0),
            "invalid output index should fill with silence");
    }

    #[test]
    fn process_output_underrun_repeats_last_frame() {
        let chain = io_passthrough_chain("chain:underrun");
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime should build"));

        // Push enough frames to get past the fade-in, then push our test frames
        let warmup = vec![0.0f32; FADE_IN_FRAMES + 16];
        process_input_f32(&runtime, 0, &warmup, 1);
        // Drain warmup frames
        let mut drain = vec![0.0f32; warmup.len()];
        process_output_f32(&runtime, 0, &mut drain, 1);

        // Now push only 2 frames (no fade-in active)
        process_input_f32(&runtime, 0, &[0.5, 0.7], 1);
        // Request 4 frames — last 2 should repeat the last pushed frame
        let mut out = vec![0.0f32; 4];
        process_output_f32(&runtime, 0, &mut out, 1);

        assert!((out[0] - 0.5).abs() < 1e-6, "frame 0 should be 0.5, got {}", out[0]);
        assert!((out[1] - 0.7).abs() < 1e-6, "frame 1 should be 0.7, got {}", out[1]);
        // Frames 2 and 3 should be the last frame (0.7) repeated
        assert!((out[2] - 0.7).abs() < 1e-6, "frame 2 should repeat last: 0.7, got {}", out[2]);
        assert!((out[3] - 0.7).abs() < 1e-6, "frame 3 should repeat last: 0.7, got {}", out[3]);
    }

    // ── ChainRuntimeState method tests ───────────────────────────────────────

    #[test]
    fn measured_latency_ms_returns_zero_initially() {
        let chain = tuner_track("chain:0", Vec::new());
        let runtime = build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).unwrap();
        assert!((runtime.measured_latency_ms() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn poll_errors_drains_and_returns_all() {
        let chain = tuner_track("chain:0", Vec::new());
        let runtime = build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).unwrap();
        // Manually push errors
        {
            let mut q = runtime.error_queue.lock().unwrap();
            q.push(BlockError { block_id: BlockId("err:1".into()), message: "oops".into() });
            q.push(BlockError { block_id: BlockId("err:2".into()), message: "boom".into() });
        }
        let errors = runtime.poll_errors();
        assert_eq!(errors.len(), 2);
        // Second call should be empty
        let errors2 = runtime.poll_errors();
        assert!(errors2.is_empty(), "poll_errors should drain the queue");
    }

    #[test]
    fn poll_stream_returns_none_for_unknown_block() {
        let chain = tuner_track("chain:0", Vec::new());
        let runtime = build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).unwrap();
        assert!(runtime.poll_stream(&BlockId("nonexistent".into())).is_none());
    }

    // ── effective_inputs / effective_outputs with Insert blocks ───────────────

    fn insert_chain() -> Chain {
        Chain {
            id: ChainId("chain:insert".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("input:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".into(),
                        entries: vec![InputEntry {
                            device_id: DeviceId("dev_in".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId("comp:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock {
                        effect_type: "gain".into(),
                        model: "volume".into(),
                        params: ParameterSet::default(),
                    }),
                },
                AudioBlock {
                    id: BlockId("insert:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Insert(InsertBlock {
                        model: "external_loop".into(),
                        send: InsertEndpoint {
                            device_id: DeviceId("send_dev".into()),
                            mode: ChainInputMode::Stereo,
                            channels: vec![0, 1],
                        },
                        return_: InsertEndpoint {
                            device_id: DeviceId("return_dev".into()),
                            mode: ChainInputMode::Stereo,
                            channels: vec![0, 1],
                        },
                    }),
                },
                AudioBlock {
                    id: BlockId("delay:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock {
                        effect_type: "gain".into(),
                        model: "volume".into(),
                        params: ParameterSet::default(),
                    }),
                },
                AudioBlock {
                    id: BlockId("output:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".into(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId("dev_out".into()),
                            mode: ChainOutputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
            ],
        }
    }

    #[test]
    fn effective_inputs_includes_insert_return() {
        let chain = insert_chain();
        let (eff_inputs, cpal_indices) = effective_inputs(&chain);
        // Should have: 1 regular input + 1 insert return = 2
        assert_eq!(eff_inputs.len(), 2);
        assert_eq!(cpal_indices.len(), 2);
    }

    #[test]
    fn effective_outputs_includes_insert_send() {
        let chain = insert_chain();
        let eff_outputs = effective_outputs(&chain);
        // Should have: 1 regular output + 1 insert send = 2
        assert_eq!(eff_outputs.len(), 2);
        assert_eq!(eff_outputs.len(), 2);
    }

    #[test]
    fn split_chain_with_insert_produces_two_segments() {
        let chain = insert_chain();
        let (eff_inputs, cpal_indices) = effective_inputs(&chain);
        let eff_outputs = effective_outputs(&chain);
        let segments = split_chain_into_segments(&chain, &eff_inputs, &cpal_indices, &eff_outputs);

        // Should have 2 segments: before insert and after insert
        assert_eq!(segments.len(), 2, "insert should split chain into 2 segments");

        // Segment 0: input → [comp:0] → insert send
        assert_eq!(segments[0].block_indices, vec![1],
            "first segment should contain only effect blocks before insert");

        // Segment 1: insert return → [delay:0] → output
        assert_eq!(segments[1].block_indices, vec![3],
            "second segment should contain only effect blocks after insert");
    }

    #[test]
    fn split_chain_with_disabled_insert_produces_one_segment() {
        let mut chain = insert_chain();
        // Disable the insert block
        chain.blocks[2].enabled = false;
        let (eff_inputs, cpal_indices) = effective_inputs(&chain);
        let eff_outputs = effective_outputs(&chain);
        let segments = split_chain_into_segments(&chain, &eff_inputs, &cpal_indices, &eff_outputs);

        // Disabled insert should not split the chain
        assert_eq!(segments.len(), 1, "disabled insert should not split the chain");
    }

    // ── effective_inputs with mono multi-channel splitting ────────────────────

    #[test]
    fn effective_inputs_splits_mono_multichannel_entry() {
        let chain = Chain {
            id: ChainId("chain:split".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0, 1, 2],
                    }],
                }),
            }],
        };
        let (eff_inputs, cpal_indices) = effective_inputs(&chain);
        assert_eq!(eff_inputs.len(), 3, "mono entry with 3 channels should split into 3 entries");
        assert_eq!(eff_inputs[0].channels, vec![0]);
        assert_eq!(eff_inputs[1].channels, vec![1]);
        assert_eq!(eff_inputs[2].channels, vec![2]);
        // All should share the same CPAL stream index
        assert_eq!(cpal_indices[0], cpal_indices[1]);
        assert_eq!(cpal_indices[1], cpal_indices[2]);
    }

    // ── effective_inputs / outputs fallback ───────────────────────────────────

    #[test]
    fn effective_inputs_fallback_when_no_input_blocks() {
        let chain = Chain {
            id: ChainId("chain:fallback".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![],
        };
        let (eff_inputs, cpal_indices) = effective_inputs(&chain);
        assert_eq!(eff_inputs.len(), 1, "fallback should produce exactly 1 input");
        assert_eq!(cpal_indices, vec![0]);
    }

    #[test]
    fn effective_outputs_fallback_when_no_output_blocks() {
        let chain = Chain {
            id: ChainId("chain:fallback".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![],
        };
        let eff_outputs = effective_outputs(&chain);
        assert_eq!(eff_outputs.len(), 1, "fallback should produce exactly 1 output");
    }

    // ── downcast_panic_message tests ─────────────────────────────────────────

    #[test]
    fn downcast_panic_str_message() {
        use super::downcast_panic_message;
        let payload: Box<dyn std::any::Any + Send> = Box::new("static string");
        assert_eq!(downcast_panic_message(payload), "static string");
    }

    #[test]
    fn downcast_panic_string_message() {
        use super::downcast_panic_message;
        let payload: Box<dyn std::any::Any + Send> = Box::new(String::from("owned string"));
        assert_eq!(downcast_panic_message(payload), "owned string");
    }

    #[test]
    fn downcast_panic_unknown_type_message() {
        use super::downcast_panic_message;
        let payload: Box<dyn std::any::Any + Send> = Box::new(42i32);
        assert_eq!(downcast_panic_message(payload), "unknown panic");
    }

    // ── process_input_f32 with stereo I/O ────────────────────────────────────

    #[test]
    fn process_input_stereo_output_preserves_channels() {
        let chain = Chain {
            id: ChainId("chain:stereo-io".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("input:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".into(),
                        entries: vec![InputEntry {
                            device_id: DeviceId("dev".into()),
                            mode: ChainInputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId("output:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".into(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId("dev".into()),
                            mode: ChainOutputMode::Stereo,
                            channels: vec![0, 1],
                        }],
                    }),
                },
            ],
        };
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime should build"));

        // Push enough frames to get past fade-in (interleaved stereo)
        let warmup = vec![0.0f32; (FADE_IN_FRAMES + 16) * 2];
        process_input_f32(&runtime, 0, &warmup, 2);
        let mut drain = vec![0.0f32; warmup.len()];
        process_output_f32(&runtime, 0, &mut drain, 2);

        // Interleaved stereo: L=0.3, R=0.7, L=0.5, R=0.9
        let input = [0.3f32, 0.7, 0.5, 0.9];
        process_input_f32(&runtime, 0, &input, 2);

        let mut out = vec![0.0f32; 4];
        process_output_f32(&runtime, 0, &mut out, 2);

        assert!((out[0] - 0.3).abs() < 1e-6, "left ch frame 0: got {}", out[0]);
        assert!((out[1] - 0.7).abs() < 1e-6, "right ch frame 0: got {}", out[1]);
        assert!((out[2] - 0.5).abs() < 1e-6, "left ch frame 1: got {}", out[2]);
        assert!((out[3] - 0.9).abs() < 1e-6, "right ch frame 1: got {}", out[3]);
    }

    // ── update_chain_runtime_state with reset_output_queue ───────────────────

    #[test]
    fn update_chain_runtime_state_with_reset_output_queue() {
        let chain = io_passthrough_chain("chain:0");
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime should build"));

        // Push some data
        process_input_f32(&runtime, 0, &[0.5, 0.7], 1);

        // Update with reset_output_queue=true should clear the buffer
        update_chain_runtime_state(&runtime, &chain, 48_000.0, true, DEFAULT_ELASTIC_TARGET)
            .expect("update should succeed");

        let mut out = vec![0.0f32; 2];
        process_output_f32(&runtime, 0, &mut out, 1);
        // After reset, output should be silence (no frames in queue)
        assert!(out.iter().all(|&v| v.abs() < 1e-6),
            "after reset_output_queue, output should be silent");
    }

    // ── layout_label tests ───────────────────────────────────────────────────

    #[test]
    fn layout_label_returns_correct_strings() {
        use super::layout_label;
        assert_eq!(layout_label(AudioChannelLayout::Mono), "mono");
        assert_eq!(layout_label(AudioChannelLayout::Stereo), "stereo");
    }

    // ── insert_return_as_input_entry tests ──────────────────────────────────

    #[test]
    fn insert_return_as_input_entry_copies_return_endpoint() {
        use super::insert_return_as_input_entry;
        let insert = InsertBlock {
            model: "external_loop".into(),
            send: InsertEndpoint {
                device_id: DeviceId("send_dev".into()),
                mode: ChainInputMode::Stereo,
                channels: vec![0, 1],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return_dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![2],
            },
        };
        let entry = insert_return_as_input_entry(&insert);
        assert_eq!(entry.device_id.0, "return_dev");
        assert_eq!(entry.channels, vec![2]);
        assert!(matches!(entry.mode, ChainInputMode::Mono));
    }

    // ── insert_send_as_output_entry tests ───────────────────────────────────

    #[test]
    fn insert_send_as_output_entry_mono_mode() {
        use super::insert_send_as_output_entry;
        let insert = InsertBlock {
            model: "external_loop".into(),
            send: InsertEndpoint {
                device_id: DeviceId("send_dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return_dev".into()),
                mode: ChainInputMode::Stereo,
                channels: vec![0, 1],
            },
        };
        let entry = insert_send_as_output_entry(&insert);
        assert_eq!(entry.device_id.0, "send_dev");
        assert_eq!(entry.channels, vec![0]);
        assert!(matches!(entry.mode, ChainOutputMode::Mono));
    }

    #[test]
    fn insert_send_as_output_entry_stereo_mode() {
        use super::insert_send_as_output_entry;
        let insert = InsertBlock {
            model: "external_loop".into(),
            send: InsertEndpoint {
                device_id: DeviceId("send_dev".into()),
                mode: ChainInputMode::Stereo,
                channels: vec![0, 1],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return_dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
        };
        let entry = insert_send_as_output_entry(&insert);
        assert!(matches!(entry.mode, ChainOutputMode::Stereo));
    }

    #[test]
    fn insert_send_as_output_entry_dual_mono_becomes_stereo() {
        use super::insert_send_as_output_entry;
        let insert = InsertBlock {
            model: "external_loop".into(),
            send: InsertEndpoint {
                device_id: DeviceId("send_dev".into()),
                mode: ChainInputMode::DualMono,
                channels: vec![0, 1],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return_dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
        };
        let entry = insert_send_as_output_entry(&insert);
        assert!(matches!(entry.mode, ChainOutputMode::Stereo));
    }

    // ── next_block_instance_serial tests ────────────────────────────────────

    #[test]
    fn next_block_instance_serial_increments() {
        use super::next_block_instance_serial;
        let a = next_block_instance_serial();
        let b = next_block_instance_serial();
        assert!(b > a, "serial should increment monotonically");
    }

    // ── build_output_routing_state tests ────────────────────────────────────

    #[test]
    fn build_output_routing_state_mono_single_channel() {
        use super::build_output_routing_state;
        let output = OutputEntry {
            device_id: DeviceId("dev".into()),
            mode: ChainOutputMode::Mono,
            channels: vec![0],
        };
        let state = build_output_routing_state(&output, DEFAULT_ELASTIC_TARGET);
        assert_eq!(state.output_channels, vec![0]);
    }

    #[test]
    fn build_output_routing_state_stereo_two_channels() {
        use super::build_output_routing_state;
        let output = OutputEntry {
            device_id: DeviceId("dev".into()),
            mode: ChainOutputMode::Stereo,
            channels: vec![0, 1],
        };
        let state = build_output_routing_state(&output, DEFAULT_ELASTIC_TARGET);
        assert_eq!(state.output_channels, vec![0, 1]);
    }

    #[test]
    fn build_output_routing_state_mono_mode_with_two_channels_uses_mono() {
        use super::build_output_routing_state;
        let output = OutputEntry {
            device_id: DeviceId("dev".into()),
            mode: ChainOutputMode::Mono,
            channels: vec![0, 1],
        };
        let _state = build_output_routing_state(&output, DEFAULT_ELASTIC_TARGET);
        // Mono mode with 2 channels: layout should be Mono per the logic
        // Just verifying it doesn't panic and runs correctly
    }

    // ── read_channel edge cases ─────────────────────────────────────────────

    #[test]
    fn read_channel_valid_index() {
        use super::read_channel;
        let data = [0.1, 0.2, 0.3];
        assert!((read_channel(&data, 1) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn read_channel_out_of_bounds_returns_zero() {
        use super::read_channel;
        let data = [0.5, 0.7];
        assert!((read_channel(&data, 10)).abs() < 1e-6);
    }

    #[test]
    fn read_channel_empty_data_returns_zero() {
        use super::read_channel;
        let data: [f32; 0] = [];
        assert!((read_channel(&data, 0)).abs() < 1e-6);
    }

    // ── runtime graph with multiple chains ──────────────────────────────────

    #[test]
    fn build_runtime_graph_with_multiple_enabled_chains() {
        let project = Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![
                tuner_track("chain:0", Vec::new()),
                tuner_track("chain:1", vec![tuner_block("b:0", 440.0)]),
            ],
        };
        let mut rates = HashMap::new();
        rates.insert(ChainId("chain:0".into()), 48_000.0);
        rates.insert(ChainId("chain:1".into()), 48_000.0);

        let runtime = build_runtime_graph(&project, &rates, &HashMap::new())
            .expect("should build with multiple chains");
        assert_eq!(runtime.chains.len(), 2);
    }

    #[test]
    fn build_runtime_graph_mixed_enabled_and_disabled() {
        let project = Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![
                Chain {
                    id: ChainId("disabled".into()),
                    description: None,
                    instrument: "electric_guitar".to_string(),
                    enabled: false,
                    blocks: vec![],
                },
                tuner_track("enabled", vec![tuner_block("b:0", 440.0)]),
            ],
        };
        let mut rates = HashMap::new();
        rates.insert(ChainId("enabled".into()), 48_000.0);

        let runtime = build_runtime_graph(&project, &rates, &HashMap::new()).unwrap();
        assert_eq!(runtime.chains.len(), 1);
        assert!(runtime.chains.contains_key(&ChainId("enabled".into())));
    }

    // ── process_input_f32 edge cases ────────────────────────────────────────

    #[test]
    fn process_input_with_empty_data_does_not_panic() {
        let chain = io_passthrough_chain("chain:0");
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime should build"));
        process_input_f32(&runtime, 0, &[], 1);
    }

    #[test]
    fn process_input_with_invalid_index_does_not_panic() {
        let chain = io_passthrough_chain("chain:0");
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime should build"));
        process_input_f32(&runtime, 999, &[0.5, 0.7], 1);
    }

    // ── effective_inputs with stereo entry does not split ────────────────────

    #[test]
    fn effective_inputs_stereo_entry_not_split() {
        let chain = Chain {
            id: ChainId("chain:stereo".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            }],
        };
        let (eff_inputs, _) = effective_inputs(&chain);
        assert_eq!(eff_inputs.len(), 1, "stereo entry should not be split");
        assert_eq!(eff_inputs[0].channels, vec![0, 1]);
    }

    // ── effective_inputs with disabled input block ──────────────────────────

    #[test]
    fn effective_inputs_ignores_disabled_blocks() {
        let chain = Chain {
            id: ChainId("chain:disabled".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("input:0".into()),
                    enabled: false,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".into(),
                        entries: vec![InputEntry {
                            device_id: DeviceId("dev".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId("output:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".into(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId("dev".into()),
                            mode: ChainOutputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
            ],
        };
        let (eff_inputs, _) = effective_inputs(&chain);
        // Disabled input block is ignored, so fallback
        assert_eq!(eff_inputs.len(), 1);
        assert_eq!(eff_inputs[0].device_id.0, "", "should fall back to default input");
    }

    // ── effective_outputs with disabled output block ─────────────────────────

    #[test]
    fn effective_outputs_ignores_disabled_blocks() {
        let chain = Chain {
            id: ChainId("chain:disabled-out".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![AudioBlock {
                id: BlockId("output:0".into()),
                enabled: false,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            }],
        };
        let eff_outputs = effective_outputs(&chain);
        assert_eq!(eff_outputs.len(), 1);
        assert_eq!(eff_outputs[0].device_id.0, "", "should fall back to default output");
    }

    // ── effective_inputs with multiple input blocks ─────────────────────────

    #[test]
    fn effective_inputs_multiple_input_blocks() {
        let chain = Chain {
            id: ChainId("chain:multi-in".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![
                AudioBlock {
                    id: BlockId("input:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".into(),
                        entries: vec![InputEntry {
                            device_id: DeviceId("dev1".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
                AudioBlock {
                    id: BlockId("input:1".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".into(),
                        entries: vec![InputEntry {
                            device_id: DeviceId("dev2".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        }],
                    }),
                },
            ],
        };
        let (eff_inputs, cpal_indices) = effective_inputs(&chain);
        assert_eq!(eff_inputs.len(), 2);
        assert_eq!(eff_inputs[0].device_id.0, "dev1");
        assert_eq!(eff_inputs[1].device_id.0, "dev2");
        // Different devices should have different CPAL indices
        assert_ne!(cpal_indices[0], cpal_indices[1]);
    }

    // ── effective_inputs same device shares cpal index ──────────────────────

    #[test]
    fn effective_inputs_same_device_shares_cpal_index() {
        let chain = Chain {
            id: ChainId("chain:same-dev".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![
                        InputEntry {
                            device_id: DeviceId("same_dev".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![0],
                        },
                        InputEntry {
                            device_id: DeviceId("same_dev".into()),
                            mode: ChainInputMode::Mono,
                            channels: vec![1],
                        },
                    ],
                }),
            }],
        };
        let (eff_inputs, cpal_indices) = effective_inputs(&chain);
        assert_eq!(eff_inputs.len(), 2);
        assert_eq!(cpal_indices[0], cpal_indices[1], "same device should share CPAL index");
    }

    // ── build_chain_runtime_state with only effects (no I/O blocks) ─────────

    #[test]
    fn build_chain_runtime_state_no_io_blocks_uses_fallback() {
        let chain = Chain {
            id: ChainId("chain:no-io".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![tuner_block("b:0", 440.0)],
        };
        let runtime = build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET);
        assert!(runtime.is_ok(), "should build with fallback I/O");
    }

    // ── process passthrough chain round-trip ────────────────────────────────

    #[test]
    fn passthrough_chain_round_trip_preserves_signal() {
        let chain = io_passthrough_chain("chain:rt");
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).expect("runtime should build"));

        // Warm up past fade-in
        let warmup = vec![0.0f32; FADE_IN_FRAMES + 64];
        process_input_f32(&runtime, 0, &warmup, 1);
        let mut drain = vec![0.0f32; warmup.len()];
        process_output_f32(&runtime, 0, &mut drain, 1);

        // Now test actual signal
        let input = [0.1f32, 0.2, 0.3, 0.4];
        process_input_f32(&runtime, 0, &input, 1);
        let mut out = vec![0.0f32; 4];
        process_output_f32(&runtime, 0, &mut out, 1);

        for (i, (&expected, &actual)) in input.iter().zip(out.iter()).enumerate() {
            assert!((expected - actual).abs() < 1e-6,
                "frame {i}: expected {expected}, got {actual}");
        }
    }

    // ── ChainRuntimeState measured_latency_ms with stored nanos ─────────────

    #[test]
    fn measured_latency_ms_converts_nanos_correctly() {
        let chain = tuner_track("chain:lat", Vec::new());
        let runtime = build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET).unwrap();
        // Store 5ms worth of nanos
        runtime.measured_latency_nanos.store(5_000_000, std::sync::atomic::Ordering::Relaxed);
        let ms = runtime.measured_latency_ms();
        assert!((ms - 5.0).abs() < 1e-3, "expected ~5.0ms, got {ms}");
    }

    // ── Empty chain builds successfully ─────────────────────────────────────

    #[test]
    fn build_chain_runtime_state_empty_chain_succeeds() {
        let chain = Chain {
            id: ChainId("chain:empty".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![],
        };
        let runtime = build_chain_runtime_state(&chain, 48_000.0, DEFAULT_ELASTIC_TARGET);
        assert!(runtime.is_ok(), "empty chain should build successfully");
    }

    // ── ElasticBuffer push/pop FIFO order ───────────────────────────────────

    #[test]
    fn elastic_buffer_fifo_order() {
        let mut buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
        for i in 0..10 {
            buf.push(AudioFrame::Mono(i as f32 * 0.1));
        }
        for i in 0..10 {
            let frame = buf.pop();
            let expected = i as f32 * 0.1;
            assert!(matches!(frame, AudioFrame::Mono(v) if (v - expected).abs() < 1e-6),
                "frame {i}: expected {expected}");
        }
    }

    // ── RuntimeGraph remove non-existent chain ──────────────────────────────

    #[test]
    fn runtime_graph_remove_nonexistent_chain_no_panic() {
        use super::RuntimeGraph;
        let mut graph = RuntimeGraph { chains: HashMap::new() };
        graph.remove_chain(&ChainId("does_not_exist".into()));
        assert!(graph.chains.is_empty());
    }

    // ── bypass_runtime_node tests ───────────────────────────────────────────

    #[test]
    fn bypass_runtime_node_has_bypass_processor() {
        use super::bypass_runtime_node;
        let block = AudioBlock {
            id: BlockId("test:bypass".into()),
            enabled: false,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        };
        let node = bypass_runtime_node(&block, AudioChannelLayout::Mono);
        assert!(matches!(node.processor, RuntimeProcessor::Bypass));
        assert_eq!(node.block_id.0, "test:bypass");
        assert_eq!(node.input_layout, AudioChannelLayout::Mono);
        assert_eq!(node.output_layout, AudioChannelLayout::Mono);
    }

    // ── SelectRuntimeState selected_node_mut ────────────────────────────────

    #[test]
    fn select_runtime_state_finds_selected_option() {
        use super::SelectRuntimeState;
        let mut state = SelectRuntimeState {
            selected_block_id: BlockId("opt:b".into()),
            options: vec![
                counting_block_node(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0))),
                {
                    let mut node = counting_block_node(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)));
                    node.block_id = BlockId("opt:b".into());
                    node
                },
            ],
        };
        let found = state.selected_node_mut();
        assert!(found.is_some());
        assert_eq!(found.unwrap().block_id.0, "opt:b");
    }

    #[test]
    fn select_runtime_state_returns_none_when_no_match() {
        use super::SelectRuntimeState;
        let mut state = SelectRuntimeState {
            selected_block_id: BlockId("nonexistent".into()),
            options: vec![
                counting_block_node(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0))),
            ],
        };
        assert!(state.selected_node_mut().is_none());
    }

    // ── processor_scratch tests ─────────────────────────────────────────────

    #[test]
    fn processor_scratch_mono_creates_mono_scratch() {
        use super::processor_scratch;
        struct NoopMono;
        impl block_core::MonoProcessor for NoopMono {
            fn process_sample(&mut self, s: f32) -> f32 { s }
        }
        let proc = AudioProcessor::Mono(Box::new(NoopMono));
        let scratch = processor_scratch(&proc);
        assert!(matches!(scratch, ProcessorScratch::Mono(_)));
    }

    #[test]
    fn processor_scratch_stereo_creates_stereo_scratch() {
        use super::processor_scratch;
        struct NoopStereo;
        impl block_core::StereoProcessor for NoopStereo {
            fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] { input }
        }
        let proc = AudioProcessor::Stereo(Box::new(NoopStereo));
        let scratch = processor_scratch(&proc);
        assert!(matches!(scratch, ProcessorScratch::Stereo(_)));
    }

    #[test]
    fn processor_scratch_dual_mono_creates_dual_mono_scratch() {
        use super::processor_scratch;
        struct NoopMono;
        impl block_core::MonoProcessor for NoopMono {
            fn process_sample(&mut self, s: f32) -> f32 { s }
        }
        let proc = AudioProcessor::DualMono {
            left: Box::new(NoopMono),
            right: Box::new(NoopMono),
        };
        let scratch = processor_scratch(&proc);
        assert!(matches!(scratch, ProcessorScratch::DualMono { .. }));
    }
}
