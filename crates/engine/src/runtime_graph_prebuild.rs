//! Responsibility: builds the fresh block nodes an in-place edit needs before it touches the live pipeline.
//!
//! #987: the in-place edit used to take every node out of the live pipelines
//! and build the new ones while they sat empty, so the audio played the raw
//! input for the whole build (a NAM load, an IR prep) and snapped back with no
//! fade — the click on every scene switch of a chain holding a VST3. Here the
//! edit is walked first against a copy of the live nodes' facts, exactly the
//! way `build_runtime_block_nodes` will walk it, and every node it will build
//! fresh is built now, while the pipeline keeps playing. What is left for the
//! swap is moving nodes, which is short enough to happen in one critical
//! section.
//!
//! A VST3 is never built here: creating an instance while live instances of
//! the same bundle are inside `process()` is the #779 crash, so an edit that
//! needs a fresh VST3 keeps the quiesced path. A `Select` keeps it too — its
//! option nodes are reused by rules this walk does not model.

use std::collections::HashMap;

use block_core::AudioChannelLayout;
use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;

use crate::runtime::ChainRuntimeState;
use crate::runtime_audio_frame::AudioProcessor;
use crate::runtime_block_builders::build_block_runtime_node;
use crate::runtime_graph_assemble::segment_bus;
use crate::runtime_segments::ChainSegment;
use crate::runtime_state::{lock_recover, BlockRuntimeNode, RuntimeProcessor};

/// What the edit's walk needs to know about one live node — copied under a
/// brief lock so the walk itself never holds it.
#[derive(Clone)]
struct LiveNodeFacts {
    block_id: BlockId,
    snapshot: AudioBlock,
    input_layout: AudioChannelLayout,
    content_mono: bool,
    output_layout: AudioChannelLayout,
    emits: Emits,
}

/// How a node changes the effective-mono content that flows through it (#588),
/// mirroring `node_emits_mono_content`.
#[derive(Clone, Copy, PartialEq)]
enum Emits {
    Mono,
    Passthrough,
    Decorrelated,
}

impl Emits {
    fn of(node: &BlockRuntimeNode) -> Self {
        match &node.processor {
            RuntimeProcessor::Audio(AudioProcessor::Mono(_)) => Emits::Mono,
            RuntimeProcessor::Audio(_)
            | RuntimeProcessor::Select(_)
            | RuntimeProcessor::Split(_) => Emits::Decorrelated,
            RuntimeProcessor::Bypass => Emits::Passthrough,
        }
    }

    fn after(self, content_mono: bool) -> bool {
        match self {
            Emits::Mono => true,
            Emits::Passthrough => content_mono,
            Emits::Decorrelated => false,
        }
    }
}

/// The nodes an in-place edit will build fresh, built ahead of the swap.
#[derive(Default)]
pub(crate) struct PrebuiltNodes {
    nodes: HashMap<BlockId, Vec<BlockRuntimeNode>>,
    /// The swap runs under the processing lock: nothing in it may log.
    under_lock: bool,
}

impl PrebuiltNodes {
    pub(crate) fn under_lock(&self) -> bool {
        self.under_lock
    }

    /// The node prebuilt for `block` at this position, if the walk predicted
    /// it: same block, same bus, same mono content.
    pub(crate) fn take(
        &mut self,
        block: &AudioBlock,
        input_layout: AudioChannelLayout,
        content_mono: bool,
    ) -> Option<BlockRuntimeNode> {
        let nodes = self.nodes.get_mut(&block.id)?;
        let at = nodes.iter().position(|node| {
            node.input_layout == input_layout
                && node.content_mono == content_mono
                && node.block_snapshot == *block
        })?;
        Some(nodes.swap_remove(at))
    }

    fn add(&mut self, node: BlockRuntimeNode) {
        self.nodes
            .entry(node.block_id.clone())
            .or_default()
            .push(node);
    }
}

/// How the edit can be applied.
pub(crate) enum Prebuild {
    /// Every fresh node is built; the swap only moves nodes.
    Ready(PrebuiltNodes),
    /// The edit needs a fresh VST3 or touches a `Select`: keep the quiesced
    /// in-place path.
    Quiesce,
}

/// Walk `segments` of `chain` against the runtime's live nodes and build every
/// node the swap will need fresh. Never touches the live pipelines beyond a
/// brief read of their nodes' facts.
pub(crate) fn prebuild_fresh_nodes(
    runtime: &ChainRuntimeState,
    chain: &Chain,
    segments: &[ChainSegment],
    segment_output_channels: &[Vec<usize>],
) -> Prebuild {
    let mut pool = live_node_facts(runtime);
    let mut prebuilt = PrebuiltNodes {
        nodes: HashMap::new(),
        under_lock: true,
    };
    for (i, segment) in segments.iter().enumerate() {
        let ids: Vec<&BlockId> = segment
            .block_indices
            .iter()
            .filter_map(|&b| chain.blocks.get(b).map(|block| &block.id))
            .collect();
        let mut reusable: HashMap<BlockId, LiveNodeFacts> = take_facts(&mut pool, i, &ids)
            .into_iter()
            .map(|facts| (facts.block_id.clone(), facts))
            .collect();
        let (mut layout, mut content_mono) = segment_bus(
            &segment.input,
            segment_output_channels
                .get(i)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        );
        for block in segment
            .block_indices
            .iter()
            .filter_map(|&b| chain.blocks.get(b))
        {
            if !block.enabled {
                if let Some(facts) = reusable.remove(&block.id) {
                    content_mono = facts.emits.after(content_mono);
                }
                continue;
            }
            match &block.kind {
                AudioBlockKind::Input(_)
                | AudioBlockKind::Output(_)
                | AudioBlockKind::Insert(_) => continue,
                AudioBlockKind::Select(_) => return Prebuild::Quiesce,
                _ => {}
            }
            let reused = reusable
                .remove(&block.id)
                .filter(|facts| facts.input_layout == layout && facts.content_mono == content_mono);
            if let Some(facts) = reused {
                match reuse(&facts, block) {
                    Reuse::Kept => {
                        layout = facts.output_layout;
                        content_mono = facts.emits.after(content_mono);
                        continue;
                    }
                    Reuse::Retuned if is_vst3(block) => {
                        // A VST3 always retunes in place (#779).
                        layout = facts.output_layout;
                        content_mono = facts.emits.after(content_mono);
                        continue;
                    }
                    // A retune that may be refused: build the fallback now, so
                    // a refusal never builds under the swap. The walk follows
                    // the reused node, which is what a retune that holds plays.
                    Reuse::Retuned => {
                        if let Ok(node) = build_block_runtime_node(
                            chain,
                            block,
                            layout,
                            content_mono,
                            runtime.sample_rate(),
                        ) {
                            prebuilt.add(node);
                        }
                        layout = facts.output_layout;
                        content_mono = facts.emits.after(content_mono);
                        continue;
                    }
                    Reuse::Rebuilt => {}
                }
            }
            if is_vst3(block) {
                return Prebuild::Quiesce;
            }
            log::info!(
                "[engine] prebuild block {:?} (id={}) for a live edit",
                block.model_ref().map(|m| m.model),
                block.id.0
            );
            match build_block_runtime_node(
                chain,
                block,
                layout,
                content_mono,
                runtime.sample_rate(),
            ) {
                Ok(node) => {
                    layout = node.output_layout;
                    content_mono = Emits::of(&node).after(content_mono);
                    prebuilt.add(node);
                }
                // The swap builds it again and faults it exactly as today.
                Err(_) => return Prebuild::Quiesce,
            }
        }
    }
    Prebuild::Ready(prebuilt)
}

/// What `try_reuse_block_node` will do with a live node the edit keeps on the
/// same bus and mono content.
enum Reuse {
    /// Reused as it is (same block, or only `enabled` flipped).
    Kept,
    /// Reused if the processor accepts the new parameters in place.
    Retuned,
    /// Built fresh.
    Rebuilt,
}

fn reuse(facts: &LiveNodeFacts, block: &AudioBlock) -> Reuse {
    if facts.snapshot == *block {
        return Reuse::Kept;
    }
    let mut same_but_enabled = facts.snapshot.clone();
    same_but_enabled.enabled = block.enabled;
    if same_but_enabled == *block {
        // A node born disabled has no processor: enabling it builds one.
        return if facts.emits == Emits::Passthrough {
            Reuse::Rebuilt
        } else {
            Reuse::Kept
        };
    }
    let (Some(previous), Some(next)) = (facts.snapshot.model_ref(), block.model_ref()) else {
        return Reuse::Rebuilt;
    };
    let same_model = std::mem::discriminant(&facts.snapshot.kind)
        == std::mem::discriminant(&block.kind)
        && previous.effect_type == next.effect_type
        && previous.model == next.model;
    if same_model && facts.snapshot.enabled && facts.emits != Emits::Passthrough {
        Reuse::Retuned
    } else {
        Reuse::Rebuilt
    }
}

fn is_vst3(block: &AudioBlock) -> bool {
    block
        .model_ref()
        .is_some_and(|model| model.effect_type == block_core::EFFECT_TYPE_VST3)
}

/// The facts of every live node, per pipeline, read under one brief lock.
fn live_node_facts(runtime: &ChainRuntimeState) -> Vec<Vec<LiveNodeFacts>> {
    let processing = lock_recover(&runtime.processing, "chain runtime");
    processing
        .input_states
        .iter()
        .map(|state| {
            state
                .blocks
                .iter()
                .map(|node| LiveNodeFacts {
                    block_id: node.block_id.clone(),
                    snapshot: node.block_snapshot.clone(),
                    input_layout: node.input_layout,
                    content_mono: node.content_mono,
                    output_layout: node.output_layout,
                    emits: Emits::of(node),
                })
                .collect()
        })
        .collect()
}

/// `take_reusable_nodes`, on facts: one per id, own pipeline first.
fn take_facts(
    pool: &mut [Vec<LiveNodeFacts>],
    preferred: usize,
    ids: &[&BlockId],
) -> Vec<LiveNodeFacts> {
    let order: Vec<usize> = std::iter::once(preferred)
        .filter(|&p| p < pool.len())
        .chain((0..pool.len()).filter(|&i| i != preferred))
        .collect();
    let mut taken = Vec::with_capacity(ids.len());
    for id in ids {
        for &pipeline in &order {
            if let Some(at) = pool[pipeline]
                .iter()
                .position(|facts| &&facts.block_id == id)
            {
                taken.push(pool[pipeline].remove(at));
                break;
            }
        }
    }
    taken
}
