//! Responsibility: builds the runtime node a block turns into.
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
use block_core::{AudioChannelLayout, BlockProcessor};
use block_nam::build_nam_processor_for_layout;
use domain::ids::BlockId;
use project::block::{AudioBlockKind, NamBlock, SelectBlock};
use project::chain::Chain;

use crate::runtime::FADE_IN_FRAMES;
use crate::runtime_audio_frame::{AudioProcessor, ProcessorScratch};
use crate::runtime_block_core::build_core_block_runtime_node;
use crate::runtime_graph_prebuild::PrebuiltNodes;
use crate::runtime_state::{
    BlockRuntimeNode, FadeState, ProcessorBuildOutcome, RuntimeProcessor, SelectRuntimeState,
};

// Declared here, not in `lib.rs`, to keep the crate router under 100 lines (#328).
#[path = "runtime_block_reuse.rs"]
pub(crate) mod reuse;

use reuse::{reuse_pool, try_reuse_block_node};

static NEXT_BLOCK_INSTANCE_SERIAL: AtomicU64 = AtomicU64::new(1);

/// Whether the signal LEAVING `node` is effectively mono (L == R), given
/// whether the signal entering it was. Issue #588: a mono processor always
/// emits `Stereo([s, s])`, a bypass preserves whatever flowed in, and any
/// genuinely stereo processor (dual-mono with independent channels, true
/// stereo, mono→stereo) may decorrelate the channels. A `Select` is treated
/// conservatively as decorrelating (its chosen branch is opaque here).
fn node_emits_mono_content(node: &BlockRuntimeNode, input_content_mono: bool) -> bool {
    match &node.processor {
        RuntimeProcessor::Audio(AudioProcessor::Mono(_)) => true,
        RuntimeProcessor::Audio(_) => false,
        RuntimeProcessor::Bypass => input_content_mono,
        RuntimeProcessor::Select(_) => false,
        // A split mixes two paths and may pan them apart: stereo content.
        RuntimeProcessor::Split(_) => false,
    }
}

pub(crate) fn build_runtime_block_nodes(
    chain: &Chain,
    input_layout: AudioChannelLayout,
    source_is_mono: bool,
    sample_rate: f32,
    existing: Option<Vec<BlockRuntimeNode>>,
    block_indices: Option<&[usize]>,
) -> Result<(Vec<BlockRuntimeNode>, AudioChannelLayout)> {
    build_runtime_block_nodes_with(
        chain,
        input_layout,
        source_is_mono,
        sample_rate,
        existing,
        block_indices,
        None,
    )
}

/// [`build_runtime_block_nodes`], taking a node that has to be built fresh
/// from `prebuilt` when the in-place edit already built it (#987).
pub(crate) fn build_runtime_block_nodes_with(
    chain: &Chain,
    input_layout: AudioChannelLayout,
    source_is_mono: bool,
    sample_rate: f32,
    existing: Option<Vec<BlockRuntimeNode>>,
    block_indices: Option<&[usize]>,
    prebuilt: Option<&mut PrebuiltNodes>,
) -> Result<(Vec<BlockRuntimeNode>, AudioChannelLayout)> {
    let mut reusable_nodes = reuse_pool(existing);
    // If block_indices is provided, iterate only those blocks; otherwise iterate all
    let block_iter: Vec<&project::block::AudioBlock> = match block_indices {
        Some(indices) => indices
            .iter()
            .filter_map(|&i| chain.blocks.get(i))
            .collect(),
        None => chain.blocks.iter().collect(),
    };
    let (blocks, layout, _) = build_nodes_for(
        chain,
        &block_iter,
        input_layout,
        source_is_mono,
        sample_rate,
        &mut reusable_nodes,
        prebuilt,
    )?;
    Ok((blocks, layout))
}

/// Build the runtime nodes of `block_iter` in order, drawing old nodes from
/// `reusable_nodes`. Returns the nodes, the layout leaving the last one and
/// whether that signal is still effectively mono (#588). The chain builder
/// calls it for the chain's blocks; the split builder calls it per path (#328).
pub(crate) fn build_nodes_for(
    chain: &Chain,
    block_iter: &[&project::block::AudioBlock],
    input_layout: AudioChannelLayout,
    source_is_mono: bool,
    sample_rate: f32,
    reusable_nodes: &mut HashMap<BlockId, BlockRuntimeNode>,
    mut prebuilt: Option<&mut PrebuiltNodes>,
) -> Result<(Vec<BlockRuntimeNode>, AudioChannelLayout, bool)> {
    // A live in-place edit hands its prebuilt nodes in; an initial build never.
    let hand_over = prebuilt.is_some();
    // #987: the live edit's swap runs under the processing lock — no logging.
    let quiet = prebuilt.as_deref().is_some_and(PrebuiltNodes::under_lock);
    let mut blocks = Vec::new();
    let mut current_layout = input_layout;
    // Issue #588: track whether the signal reaching the current position is
    // still effectively mono (a mono source broadcast to identical stereo
    // channels). Starts from the source layout and is cleared the moment a
    // block produces genuine stereo.
    let mut content_mono = source_is_mono;

    for &block in block_iter {
        // #328: a split builds its own node, on or off — a split switched off
        // must fade out through its real paths, not through an empty shell.
        if let AudioBlockKind::Split(split) = &block.kind {
            let node = crate::runtime_split::builder::build_split_runtime_node(
                chain,
                block,
                split,
                current_layout,
                content_mono,
                sample_rate,
                reusable_nodes,
            )?;
            current_layout = node.output_layout;
            content_mono = node_emits_mono_content(&node, content_mono);
            blocks.push(node);
            continue;
        }
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
                blocks.push(bypass_runtime_node(block, current_layout, content_mono));
            }
            content_mono = node_emits_mono_content(blocks.last().unwrap(), content_mono);
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
                content_mono,
                sample_rate,
                existing_select_node,
            )?;
            current_layout = node.output_layout;
            content_mono = node_emits_mono_content(&node, content_mono);
            blocks.push(node);
            continue;
        }
        let replaced = match try_reuse_block_node(
            reusable_nodes,
            block,
            current_layout,
            content_mono,
            sample_rate,
            quiet,
        ) {
            Ok(node) => {
                if !quiet {
                    log::info!(
                        "[engine] reuse block {:?} (id={})",
                        block.model_ref().map(|m| m.model),
                        block.id.0
                    );
                }
                current_layout = node.output_layout;
                content_mono = node_emits_mono_content(&node, content_mono);
                blocks.push(node);
                continue;
            }
            Err(replaced) => replaced,
        };

        if !quiet {
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
        }
        let ready = prebuilt
            .as_deref_mut()
            .and_then(|nodes| nodes.take(block, current_layout, content_mono));
        let built = match ready {
            Some(node) => Ok(node),
            None => {
                build_block_runtime_node(chain, block, current_layout, content_mono, sample_rate)
            }
        };
        match built {
            Ok(mut node) => {
                // #987: on a live edit the fresh node takes over from the one
                // it replaces instead of cutting to its own cold start.
                if hand_over {
                    crate::runtime_node_handover::begin_handover(&mut node, replaced);
                }
                current_layout = node.output_layout;
                content_mono = node_emits_mono_content(&node, content_mono);
                blocks.push(node);
            }
            Err(e) => {
                // Don't fail the whole chain — bypass this block and keep going,
                // but record the reason so offline-render callers (and any
                // diagnostic surface) can refuse to claim success. Issue #574:
                // without this, the failure was log-only and invisible to the
                // CLI, producing misleading WAV output for different presets.
                let reason = e.to_string();
                log::error!(
                    "[engine] block {:?} (id={}) build failed: {reason} — inserting faulted bypass",
                    block.model_ref().map(|m| m.model.to_string()),
                    block.id.0
                );
                let mut node = bypass_runtime_node(block, current_layout, content_mono);
                node.faulted = true;
                node.fault_reason = Some(reason);
                // A faulted block is bypassed → passthrough preserves content.
                blocks.push(node);
            }
        }
    }

    Ok((blocks, current_layout, content_mono))
}

pub(crate) fn build_block_runtime_node(
    chain: &Chain,
    block: &project::block::AudioBlock,
    input_layout: AudioChannelLayout,
    content_mono: bool,
    sample_rate: f32,
) -> Result<BlockRuntimeNode> {
    Ok(match &block.kind {
        _ if !block.enabled => bypass_runtime_node(block, input_layout, content_mono),
        AudioBlockKind::Nam(stage) => audio_block_runtime_node(
            block,
            input_layout,
            content_mono,
            build_nam_audio_processor(chain, stage, input_layout, content_mono, sample_rate)?,
        ),
        AudioBlockKind::Core(core) => build_core_block_runtime_node(
            chain,
            block,
            core,
            input_layout,
            content_mono,
            sample_rate,
        )?,
        AudioBlockKind::Select(select) => build_select_runtime_node(
            chain,
            block,
            select,
            input_layout,
            content_mono,
            sample_rate,
            None,
        )?,
        // Input/Output/Insert blocks are routing-only; they don't process audio in the block chain
        AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => {
            bypass_runtime_node(block, input_layout, content_mono)
        }
        AudioBlockKind::Split(split) => crate::runtime_split::builder::build_split_runtime_node(
            chain,
            block,
            split,
            input_layout,
            content_mono,
            sample_rate,
            &mut HashMap::new(),
        )?,
    })
}

fn build_select_runtime_node(
    chain: &Chain,
    block: &project::block::AudioBlock,
    select: &SelectBlock,
    input_layout: AudioChannelLayout,
    content_mono: bool,
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
        let option_node = if let Ok(node) = try_reuse_block_node(
            &mut reusable_option_nodes,
            option,
            input_layout,
            content_mono,
            sample_rate,
            false,
        ) {
            node
        } else {
            build_block_runtime_node(chain, option, input_layout, content_mono, sample_rate)?
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
        content_mono,
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
        fade_dry_buffer: Vec::new(),
        faulted: false,
        fault_reason: None,
        handover: None,
    })
}

pub(crate) fn bypass_runtime_node(
    block: &project::block::AudioBlock,
    input_layout: AudioChannelLayout,
    content_mono: bool,
) -> BlockRuntimeNode {
    BlockRuntimeNode {
        instance_serial: next_block_instance_serial(),
        block_id: block.id.clone(),
        block_snapshot: block.clone(),
        input_layout,
        content_mono,
        output_layout: input_layout,
        scratch: ProcessorScratch::None,
        processor: RuntimeProcessor::Bypass,
        stream_handle: None,
        fade_state: FadeState::Bypassed,
        fade_dry_buffer: Vec::new(),
        faulted: false,
        fault_reason: None,
        handover: None,
    }
}

pub(crate) fn audio_block_runtime_node(
    block: &project::block::AudioBlock,
    input_layout: AudioChannelLayout,
    content_mono: bool,
    outcome: ProcessorBuildOutcome,
) -> BlockRuntimeNode {
    let scratch = processor_scratch(&outcome.processor);
    BlockRuntimeNode {
        instance_serial: next_block_instance_serial(),
        block_id: block.id.clone(),
        block_snapshot: block.clone(),
        input_layout,
        content_mono,
        output_layout: outcome.output_layout,
        scratch,
        processor: RuntimeProcessor::Audio(outcome.processor),
        stream_handle: outcome.stream_handle,
        fade_state: FadeState::FadingIn {
            frames_remaining: FADE_IN_FRAMES,
        },
        fade_dry_buffer: Vec::new(),
        faulted: false,
        fault_reason: None,
        handover: None,
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

fn build_nam_audio_processor(
    chain: &Chain,
    stage: &NamBlock,
    input_layout: AudioChannelLayout,
    content_mono: bool,
    sample_rate: f32,
) -> Result<ProcessorBuildOutcome> {
    crate::runtime_processor_model::build_audio_processor_for_model(
        chain,
        block_core::EFFECT_TYPE_NAM,
        &stage.model,
        input_layout,
        content_mono,
        |layout| build_nam_processor_via_dispatch(&stage.model, &stage.params, sample_rate, layout),
    )
}

/// Resolve a NAM block via the plugin loader (issue #574) and fall back
/// to the legacy `model_path`-in-params path only for callers that
/// inject the path themselves. Without this, YAML presets never built
/// because they don't carry `model_path`.
fn build_nam_processor_via_dispatch(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    if let Some(package) = plugin_loader::registry::find(model) {
        return package.build_processor(params, sample_rate, layout);
    }
    if params.get_string("model_path").is_some() {
        return build_nam_processor_for_layout(model, params, sample_rate, layout);
    }
    Err(anyhow!(
        "no NAM plugin package registered for model '{model}' and no `model_path` in params"
    ))
}

pub(crate) fn next_block_instance_serial() -> u64 {
    NEXT_BLOCK_INSTANCE_SERIAL.fetch_add(1, Ordering::Relaxed)
}
