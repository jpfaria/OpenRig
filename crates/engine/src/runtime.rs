use anyhow::{anyhow, Result};
use domain::ids::{BlockId, ChainId};
use project::block::{
    schema_for_block_model, AudioBlockKind, CoreBlockKind, NamBlock, SelectBlock,
};
use project::param::ParameterSet;
use project::project::Project;
use project::chain::{Chain, ChainOutputMixdown};
use block_amp_combo::build_amp_combo_processor_for_layout;
use block_amp_head::build_amp_head_processor_for_layout;
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
}

struct ChainProcessingState {
    input_layout: AudioChannelLayout,
    input_channels: Vec<usize>,
    blocks: Vec<BlockRuntimeNode>,
    frame_buffer: Vec<AudioFrame>,
    tuner_samples: Vec<f32>,
}

struct ChainOutputState {
    output_layout: AudioChannelLayout,
    output_channels: Vec<usize>,
    output_mixdown: ChainOutputMixdown,
    processed_frames: VecDeque<AudioFrame>,
}

enum RuntimeProcessor {
    Audio(AudioProcessor),
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
    let input_layout = layout_from_channels(chain.input_channels.len())?;
    let (blocks, output_layout) =
        build_runtime_block_nodes(chain, input_layout, sample_rate, None)?;

    Ok(ChainRuntimeState {
        processing: Mutex::new(ChainProcessingState {
            input_layout,
            input_channels: chain.input_channels.clone(),
            blocks,
            frame_buffer: Vec::new(),
            tuner_samples: Vec::new(),
        }),
        output: Mutex::new(ChainOutputState {
            output_layout,
            output_channels: chain.output_channels.clone(),
            output_mixdown: chain.output_mixdown,
            processed_frames: VecDeque::with_capacity(MAX_BUFFERED_OUTPUT_FRAMES),
        }),
    })
}

pub fn update_chain_runtime_state(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    sample_rate: f32,
    reset_output_queue: bool,
) -> Result<()> {
    let input_layout = layout_from_channels(chain.input_channels.len())?;
    let mut processing = runtime.processing.lock().expect("chain runtime poisoned");
    let existing = std::mem::take(&mut processing.blocks);
    let (blocks, output_layout) =
        build_runtime_block_nodes(chain, input_layout, sample_rate, Some(existing))?;
    processing.input_layout = input_layout;
    processing.input_channels = chain.input_channels.clone();
    processing.blocks = blocks;
    processing.frame_buffer.clear();
    processing.tuner_samples.clear();
    drop(processing);

    let mut output = runtime.output.lock().expect("chain runtime poisoned");
    output.output_layout = output_layout;
    output.output_channels = chain.output_channels.clone();
    output.output_mixdown = chain.output_mixdown;
    if reset_output_queue {
        output.processed_frames.clear();
    } else {
        trim_output_queue(&mut output.processed_frames);
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
) -> Result<(Vec<BlockRuntimeNode>, AudioChannelLayout)> {
    let mut blocks = Vec::new();
    let mut current_layout = input_layout;
    let mut reusable_nodes = existing
        .unwrap_or_default()
        .into_iter()
        .map(|node| (node.block_id.clone(), node))
        .collect::<HashMap<_, _>>();

    for block in &chain.blocks {
        if !block.enabled {
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
            current_layout = node.output_layout;
            blocks.push(node);
            continue;
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
    let node = reusable_nodes.remove(&block.id)?;
    if node.block_snapshot == *block && node.input_layout == current_layout {
        Some(node)
    } else {
        None
    }
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
            build_nam_audio_processor(chain, stage, input_layout)?,
        ),
        AudioBlockKind::Core(core) => match &core.kind {
            CoreBlockKind::AmpHead(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "amp_head", &stage.model, input_layout, |layout| {
                    build_amp_head_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::AmpCombo(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "amp_combo", &stage.model, input_layout, |layout| {
                    build_amp_combo_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::FullRig(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "full_rig", &stage.model, input_layout, |layout| {
                    build_full_rig_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::Cab(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "cab", &stage.model, input_layout, |layout| {
                    build_cab_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::Ir(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "ir", &stage.model, input_layout, |layout| {
                    build_ir_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::Drive(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "gain", &stage.model, input_layout, |layout| {
                    build_gain_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::Delay(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "delay", &stage.model, input_layout, |layout| {
                    build_delay_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::Reverb(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "reverb", &stage.model, input_layout, |layout| {
                    build_reverb_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::Tuner(stage) => BlockRuntimeNode {
                instance_serial: next_block_instance_serial(),
                block_id: block.id.clone(),
                block_snapshot: block.clone(),
                input_layout,
                output_layout: input_layout,
                scratch: ProcessorScratch::Mono(Vec::new()),
                processor: RuntimeProcessor::Tuner(build_utility_processor(
                    &stage.model,
                    &stage.params,
                    sample_rate.round() as usize,
                )?),
            },
            CoreBlockKind::Compressor(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "dynamics", &stage.model, input_layout, |layout| {
                    build_dynamics_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::Gate(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "dynamics", &stage.model, input_layout, |layout| {
                    build_dynamics_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::Eq(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "filter", &stage.model, input_layout, |layout| {
                    build_filter_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::Wah(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "wah", &stage.model, input_layout, |layout| {
                    build_wah_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
            CoreBlockKind::Tremolo(stage) => audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(chain, "modulation", &stage.model, input_layout, |layout| {
                    build_modulation_processor_for_layout(&stage.model, &stage.params, sample_rate, layout)
                })?,
            ),
        },
        AudioBlockKind::Select(select) => {
            build_select_runtime_node(chain, block, select, input_layout, sample_rate, None)?
        }
    })
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
) -> Result<ProcessorBuildOutcome> {
    let _ = (
        optional_string(&stage.params, "ir_path"),
        required_string(&stage.params, "model_path")?,
    );
    build_audio_processor_for_model(chain, "nam", &stage.model, input_layout, |layout| {
        build_nam_processor_for_layout(&stage.model, &stage.params, layout)
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

pub fn process_input_f32(runtime: &Arc<ChainRuntimeState>, data: &[f32], input_total_channels: usize) {
    let num_frames = data.len() / input_total_channels;
    let mut processing = runtime.processing.lock().expect("chain runtime poisoned");
    let ChainProcessingState {
        input_layout,
        input_channels,
        blocks,
        frame_buffer,
        tuner_samples,
    } = &mut *processing;
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
        let chain_frame = read_input_frame(*input_layout, input_channels, frame);
        if tuner_enabled {
            tuner_samples.push(chain_frame.mono_mix());
        }
        frame_buffer.push(chain_frame);
    }

    if tuner_enabled && !tuner_samples.is_empty() {
        for block in blocks.iter_mut() {
            process_tuners(block, tuner_samples);
        }
    }

    for block in blocks.iter_mut() {
        process_audio_block(block, frame_buffer.as_mut_slice());
    }

    let mut output = runtime.output.lock().expect("chain runtime poisoned");
    output.processed_frames.extend(frame_buffer.drain(..));
    trim_output_queue(&mut output.processed_frames);
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
    out: &mut [f32],
    output_total_channels: usize,
) {
    let mut output_state = runtime.output.lock().expect("chain runtime poisoned");
    let num_frames = out.len() / output_total_channels;

    for frame in out.chunks_mut(output_total_channels).take(num_frames) {
        frame.fill(0.0);
        let processed = output_state
            .processed_frames
            .pop_front()
            .unwrap_or_else(|| silent_frame(output_state.output_layout));
        write_output_frame(
            processed,
            &output_state.output_channels,
            frame,
            output_state.output_mixdown,
        );
    }
}

fn trim_output_queue(queue: &mut VecDeque<AudioFrame>) {
    while queue.len() > MAX_BUFFERED_OUTPUT_FRAMES {
        queue.pop_front();
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

fn write_output_frame(
    chain_frame: AudioFrame,
    output_channels: &[usize],
    frame: &mut [f32],
    mixdown: ChainOutputMixdown,
) {
    match chain_frame {
        AudioFrame::Mono(sample) => {
            for &channel_index in output_channels {
                if let Some(dst) = frame.get_mut(channel_index) {
                    *dst = sample;
                }
            }
        }
        AudioFrame::Stereo([left, right]) => match output_channels {
            [] => {}
            [channel_index] => {
                if let Some(dst) = frame.get_mut(*channel_index) {
                    *dst = apply_mixdown(mixdown, left, right);
                }
            }
            [left_channel, right_channel, ..] => {
                if let Some(dst) = frame.get_mut(*left_channel) {
                    *dst = left;
                }
                if let Some(dst) = frame.get_mut(*right_channel) {
                    *dst = right;
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
    use block_amp_head::supported_models as supported_amp_head_models;
    use block_cab::{cab_backend_kind, supported_models as supported_cab_models, CabBackendKind};
    use block_delay::supported_models as supported_delay_models;
    use block_dyn::compressor_supported_models;
    use block_reverb::supported_models as supported_reverb_models;
    use block_util::supported_models as supported_tuner_models;
    use domain::ids::{BlockId, DeviceId, ChainId};
    use domain::value_objects::ParameterValue;
    use project::block::{
        AmpHeadBlock, AudioBlock, AudioBlockKind, CabBlock, CompressorBlock, CoreBlock,
        CoreBlockKind, DelayBlock, ReverbBlock, SelectBlock, TunerBlock, schema_for_block_model,
    };
    use project::param::ParameterSet;
    use project::project::Project;
    use project::chain::{Chain, ChainOutputMixdown};
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
                enabled: true,
                input_device_id: DeviceId("input-device".into()),
                input_channels: vec![0],
                output_device_id: DeviceId("output-device".into()),
                output_channels: vec![0],
                blocks: vec![AudioBlock {
                    id: BlockId("chain:0:block:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock {
                        kind: CoreBlockKind::Cab(CabBlock {
                            model,
                            params,
                        }),
                    }),
                }],
                output_mixdown: ChainOutputMixdown::Average,
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
                enabled: true,
                input_device_id: DeviceId("input-device".into()),
                input_channels: vec![0],
                output_device_id: DeviceId("output-device".into()),
                output_channels: vec![0],
                blocks: vec![AudioBlock {
                    id: BlockId("chain:0:block:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Core(CoreBlock {
                        kind: CoreBlockKind::Cab(CabBlock {
                            model,
                            params,
                        }),
                    }),
                }],
                output_mixdown: ChainOutputMixdown::Average,
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
                .blocks
                .iter()
                .map(|block| block.instance_serial)
                .collect::<Vec<_>>()
        };

        if let AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Tuner(stage),
        }) = &mut chain.blocks[1].kind
        {
            stage
                .params
                .insert("reference_hz", ParameterValue::Float(432.0));
        }

        update_chain_runtime_state(&runtime, &chain, 48_000.0, false)
            .expect("runtime update should succeed");

        let updated_serials = {
            let locked = runtime.processing.lock().expect("runtime poisoned");
            locked
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
                .blocks
                .iter()
                .map(|block| (block.block_id.clone(), block.instance_serial))
                .collect::<HashMap<_, _>>()
        };

        chain.blocks.swap(0, 1);

        update_chain_runtime_state(&runtime, &chain, 48_000.0, false)
            .expect("runtime update should succeed");

        let reordered = runtime.processing.lock().expect("runtime poisoned");
        assert_eq!(reordered.blocks.len(), 2);
        for block in &reordered.blocks {
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

        process_input_f32(&runtime, &input, 1);

        let output = runtime.output.lock().expect("runtime poisoned");
        assert_eq!(output.processed_frames.len(), MAX_BUFFERED_OUTPUT_FRAMES);
    }

    #[test]
    fn process_output_drains_buffered_frames() {
        let chain = tuner_track("chain:0", Vec::new());
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));

        process_input_f32(&runtime, &[0.25, 0.5, 0.75, 1.0], 1);

        let mut out = vec![0.0f32; 4];
        process_output_f32(&runtime, &mut out, 1);

        assert_eq!(out, vec![0.25, 0.5, 0.75, 1.0]);
        let output = runtime.output.lock().expect("runtime poisoned");
        assert!(output.processed_frames.is_empty());
    }

    #[test]
    fn dual_mono_chain_does_not_leak_left_into_right() {
        let chain = Chain {
            id: ChainId("chain:stereo".into()),
            description: Some("Stereo isolation".into()),
            enabled: true,
            input_device_id: DeviceId("input-device".into()),
            input_channels: vec![0, 1],
            output_device_id: DeviceId("output-device".into()),
            output_channels: vec![0, 1],
            blocks: vec![
                compressor_block("chain:stereo:block:0"),
                amp_head_block("chain:stereo:block:1"),
                native_cab_block("chain:stereo:block:2"),
                reverb_block("chain:stereo:block:3"),
            ],
            output_mixdown: ChainOutputMixdown::Average,
        };
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));

        let mut input = vec![0.0f32; 256 * 2];
        for frame in input.chunks_mut(2) {
            frame[0] = 0.25;
            frame[1] = 0.0;
        }
        process_input_f32(&runtime, &input, 2);

        let mut output = vec![0.0f32; input.len()];
        process_output_f32(&runtime, &mut output, 2);

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
            enabled: true,
            input_device_id: DeviceId("input-device".into()),
            input_channels: vec![0, 1],
            output_device_id: DeviceId("output-device".into()),
            output_channels: vec![0, 1],
            blocks: vec![
                marshall_amp_head_block("chain:asset-backed:block:0"),
                ir_cab_block("chain:asset-backed:block:1"),
                reverb_block("chain:asset-backed:block:2"),
            ],
            output_mixdown: ChainOutputMixdown::Average,
        };
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));

        let mut input = vec![0.0f32; 256 * 2];
        for frame in input.chunks_mut(2) {
            frame[0] = 0.25;
            frame[1] = 0.0;
        }
        process_input_f32(&runtime, &input, 2);

        let mut output = vec![0.0f32; input.len()];
        process_output_f32(&runtime, &mut output, 2);

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
        assert_eq!(locked.blocks.len(), 1);
    }

    #[test]
    fn update_chain_runtime_state_preserves_select_instance_when_switching_active_option() {
        let mut chain = select_delay_chain("chain:select", "delay_a");
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, 48_000.0).expect("runtime state should build"));
        let original_serial = {
            let locked = runtime.processing.lock().expect("runtime poisoned");
            locked.blocks[0].instance_serial
        };

        if let AudioBlockKind::Select(select) = &mut chain.blocks[0].kind {
            select.selected_block_id = BlockId("chain:select:block:0::delay_b".into());
        }

        update_chain_runtime_state(&runtime, &chain, 48_000.0, false)
            .expect("runtime update should succeed when switching select option");

        let updated_serial = {
            let locked = runtime.processing.lock().expect("runtime poisoned");
            locked.blocks[0].instance_serial
        };

        assert_eq!(updated_serial, original_serial);
    }

    fn tuner_track(chain_id: &str, blocks: Vec<AudioBlock>) -> Chain {
        Chain {
            id: ChainId(chain_id.into()),
            description: Some("Tuner chain".into()),
            enabled: true,
            input_device_id: DeviceId("input-device".into()),
            input_channels: vec![0],
            output_device_id: DeviceId("output-device".into()),
            output_channels: vec![0],
            blocks,
            output_mixdown: ChainOutputMixdown::Average,
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
                kind: CoreBlockKind::Tuner(TunerBlock {
                    model: tuner_model,
                    params,
                }),
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
                kind: CoreBlockKind::Compressor(CompressorBlock {
                    params: normalized_defaults("dynamics", &model),
                    model,
                }),
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
                kind: CoreBlockKind::Cab(CabBlock {
                    params: normalized_defaults("cab", &model),
                    model,
                }),
            }),
        }
    }

    fn amp_head_block(block_id: &str) -> AudioBlock {
        let model = supported_amp_head_models()
            .iter()
            .find(|model| !model.contains("marshall_jcm_800"))
            .or_else(|| supported_amp_head_models().first())
            .expect("block-amp-head must expose at least one model")
            .to_string();
        AudioBlock {
            id: BlockId(block_id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                kind: CoreBlockKind::AmpHead(AmpHeadBlock {
                    params: normalized_defaults("amp_head", &model),
                    model,
                }),
            }),
        }
    }

    fn marshall_amp_head_block(block_id: &str) -> AudioBlock {
        let model = "marshall_jcm_800_2203".to_string();
        AudioBlock {
            id: BlockId(block_id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                kind: CoreBlockKind::AmpHead(AmpHeadBlock {
                    params: normalized_defaults("amp_head", &model),
                    model,
                }),
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
                kind: CoreBlockKind::Cab(CabBlock {
                    params: normalized_defaults("cab", &model),
                    model,
                }),
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
                kind: CoreBlockKind::Reverb(ReverbBlock {
                    params: normalized_defaults("reverb", &model),
                    model,
                }),
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
            enabled: true,
            input_device_id: DeviceId("input-device".into()),
            input_channels: vec![0],
            output_device_id: DeviceId("output-device".into()),
            output_channels: vec![0],
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
            output_mixdown: ChainOutputMixdown::Average,
        }
    }

    fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
        let mut params = normalized_defaults("delay", model);
        params.insert("time_ms", ParameterValue::Float(time_ms));
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                kind: CoreBlockKind::Delay(DelayBlock {
                    params,
                    model: model.to_string(),
                }),
            }),
        }
    }
}
