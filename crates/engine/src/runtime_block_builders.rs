//! Block-level runtime node construction (slice 4 of Phase 2 issue #194).
//!
//! Setup-time only — every function in this module runs when a chain is
//! built or when an existing chain is rebuilt because a block was added,
//! removed, swapped, or had its parameters/model changed. None executes
//! on the audio thread.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, Result};

use block_core::param::ParameterSet;
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor,
};
use block_nam::build_nam_processor_for_layout;
use domain::ids::BlockId;
use project::block::{schema_for_block_model, AudioBlockKind, NamBlock, SelectBlock};
use project::chain::Chain;

use crate::runtime::{layout_label, FADE_IN_FRAMES};
use crate::runtime_audio_frame::{AudioProcessor, ProcessorScratch};
use crate::runtime_block_core::build_core_block_runtime_node;
use crate::runtime_state::{
    BlockRuntimeNode, FadeState, ProcessorBuildOutcome, RuntimeProcessor, SelectRuntimeState,
};

static NEXT_BLOCK_INSTANCE_SERIAL: AtomicU64 = AtomicU64::new(1);

pub(crate) fn build_runtime_block_nodes(
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
        Some(indices) => indices
            .iter()
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
                    node.fade_state = FadeState::FadingOut {
                        frames_remaining: FADE_IN_FRAMES,
                    };
                }
                blocks.push(node);
            } else {
                blocks.push(bypass_runtime_node(block, current_layout));
            }
            continue;
        }
        // Input/Output/Insert blocks are routing metadata; skip them in the processing chain
        if matches!(
            &block.kind,
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)
        ) {
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
            log::info!(
                "[engine] reuse block {:?} (id={})",
                block.model_ref().map(|m| m.model),
                block.id.0
            );
            current_layout = node.output_layout;
            blocks.push(node);
            continue;
        }

        log::info!(
            "[engine] rebuild block {:?} (id={}) with params:",
            block.model_ref().map(|m| m.model),
            block.id.0
        );
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
                log::error!(
                    "[engine] block {:?} (id={}) build failed: {e} — inserting faulted bypass",
                    block.model_ref().map(|m| m.model.to_string()),
                    block.id.0
                );
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
        log::debug!(
            "[engine] cannot reuse block id={}: layout changed ({:?} → {:?})",
            block.id.0,
            node.input_layout,
            current_layout
        );
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
            node.fade_state = FadeState::FadingIn {
                frames_remaining: FADE_IN_FRAMES,
            };
        }
        return Some(node);
    }
    log::info!(
        "[engine] cannot reuse block id={}: snapshot differs (params or kind changed)",
        block.id.0
    );
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
        AudioBlockKind::Core(core) => {
            build_core_block_runtime_node(chain, block, core, input_layout, sample_rate)?
        }
        AudioBlockKind::Select(select) => {
            build_select_runtime_node(chain, block, select, input_layout, sample_rate, None)?
        }
        // Input/Output/Insert blocks are routing-only; they don't process audio in the block chain
        AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => {
            bypass_runtime_node(block, input_layout)
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
        .ok_or_else(|| {
            anyhow!(
                "chain '{}' select block references unknown option",
                chain.id.0
            )
        })?;

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
            FadeState::FadingIn {
                frames_remaining: FADE_IN_FRAMES,
            }
        } else {
            FadeState::Active
        },
        faulted: false,
    })
}

pub(crate) fn bypass_runtime_node(
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

pub(crate) fn audio_block_runtime_node(
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
        fade_state: FadeState::FadingIn {
            frames_remaining: FADE_IN_FRAMES,
        },
        faulted: false,
    }
}

pub(crate) fn processor_scratch(processor: &AudioProcessor) -> ProcessorScratch {
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

pub(crate) fn build_audio_processor_for_model<F>(
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
        (ModelAudioMode::MonoOnly, _) => AudioProcessor::Mono(expect_mono_processor(
            builder(AudioChannelLayout::Mono)?,
            chain,
            effect_type,
            model,
        )?),
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
    build_audio_processor_for_model(
        chain,
        block_core::EFFECT_TYPE_NAM,
        &stage.model,
        input_layout,
        |layout| build_nam_processor_for_layout(&stage.model, &stage.params, sample_rate, layout),
    )
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

pub(crate) fn next_block_instance_serial() -> u64 {
    NEXT_BLOCK_INSTANCE_SERIAL.fetch_add(1, Ordering::Relaxed)
}
