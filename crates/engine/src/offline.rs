//! Offline render driver — drives a chain's DSP without any cpal binding.
//!
//! Reuses the same `RuntimeProcessor` and `process_buffer` as the realtime
//! callback, so a chain rendered offline is byte-identical to what the live
//! rig would emit for the same input samples and the same project. The
//! single-source-of-truth for chain DSP stays in `runtime_block_builders` —
//! this module is just an alternative driver around it.
//!
//! Used by `adapter-render` (issue #552) for headless `--render` mode.
//!
//! Scope: single chain, single segment, no I/O blocks, no multi-input
//! routing, no MIDI/automation replay. The live rig's segmentation and
//! routing live in `runtime_graph` and are bypassed here because the
//! offline driver supplies the input bus directly and consumes the output
//! bus directly — there are no device endpoints to bind to.

use anyhow::Result;
use block_core::AudioChannelLayout;
use project::chain::Chain;

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_block_builders::build_runtime_block_nodes;
use crate::runtime_state::{BlockRuntimeNode, RuntimeProcessor};

/// One block that could not be built into a runtime processor.
///
/// `render_chain` does not fail the whole render when an individual block
/// fails to build (the GUI relies on the same code path and must keep
/// running with a partial chain). Instead the block is replaced with a
/// pass-through bypass node and the failure is reported here so the
/// caller can decide whether the render is still acceptable. Issue #574:
/// the previous behavior dropped the error on the floor, producing
/// misleading WAV output where different presets rendered to identical
/// bytes because their amp blocks had silently been removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaultedBlock {
    pub block_id: String,
    pub effect_type: String,
    pub model: String,
    pub error: String,
}

/// Result of a successful offline render.
///
/// `samples` always contains a best-effort render — even when blocks
/// were silently bypassed because they could not be built. Callers that
/// need an "all blocks ran" guarantee (the CLI, regression tests) MUST
/// inspect `faulted_blocks` and treat a non-empty list as failure.
#[derive(Debug, Clone)]
pub struct RenderOutcome {
    pub samples: Vec<[f32; 2]>,
    pub faulted_blocks: Vec<FaultedBlock>,
}

/// Process a chain offline: input stereo frames in, output stereo frames out.
///
/// The output is `input.len() + tail_frames` frames long: the tail window
/// of zeros lets time-based blocks (reverb, delay) flush their state into
/// the rendered file instead of being abruptly truncated at the last
/// input sample.
///
/// `block_size` controls the internal chunking — same semantics as the
/// realtime callback's buffer size. Time-domain-stable blocks must produce
/// identical output regardless of `block_size`; if a future block does not,
/// that is a latent realtime determinism bug and is out of scope here.
pub fn render_chain(
    chain: &Chain,
    sample_rate: f32,
    input: &[[f32; 2]],
    block_size: usize,
    tail_frames: usize,
) -> Result<RenderOutcome> {
    if block_size == 0 {
        anyhow::bail!("block_size must be > 0");
    }
    // Offline render is fed an explicit stereo buffer whose channels may
    // differ; do not assume mono content (issue #588), so build the full
    // per-channel processors — byte-identical to the historical behaviour.
    let (mut nodes, _output_layout) = build_runtime_block_nodes(
        chain,
        AudioChannelLayout::Stereo,
        false,
        sample_rate,
        None,
        None,
    )?;

    let faulted_blocks = collect_faulted_blocks(&nodes);

    let total_frames = input.len() + tail_frames;
    let mut output: Vec<[f32; 2]> = Vec::with_capacity(total_frames);
    let mut chunk_buf: Vec<AudioFrame> = Vec::with_capacity(block_size);
    let mut frame_idx = 0_usize;
    while frame_idx < total_frames {
        let chunk_size = block_size.min(total_frames - frame_idx);
        chunk_buf.clear();
        for i in 0..chunk_size {
            let g = frame_idx + i;
            let pair = if g < input.len() {
                input[g]
            } else {
                [0.0_f32, 0.0_f32]
            };
            chunk_buf.push(AudioFrame::Stereo(pair));
        }
        for node in nodes.iter_mut() {
            if !node.block_snapshot.enabled {
                continue;
            }
            apply_block_offline(node, &mut chunk_buf);
        }
        for frame in chunk_buf.iter() {
            let pair = match frame {
                AudioFrame::Stereo([l, r]) => [*l, *r],
                AudioFrame::Mono(s) => [*s, *s],
            };
            output.push(pair);
        }
        frame_idx += chunk_size;
    }
    Ok(RenderOutcome {
        samples: output,
        faulted_blocks,
    })
}

/// Build the offline processing nodes for a chain ONCE (loads NAM etc.), so the
/// Tone Doctor's ablation can re-render many variants without rebuilding every
/// block and reloading the NAM from disk on each pass (#791 perf). Nodes align
/// 1:1 with `chain.blocks` in order.
pub(crate) fn build_offline_nodes(
    chain: &Chain,
    sample_rate: f32,
) -> Result<Vec<BlockRuntimeNode>> {
    let (nodes, _layout) =
        build_runtime_block_nodes(chain, AudioChannelLayout::Stereo, false, sample_rate, None, None)?;
    Ok(nodes)
}

/// Render `chain` reusing `base` nodes where the block is unchanged — only a
/// block whose snapshot differs (e.g. the one param the correction search is
/// sweeping) is rebuilt, so an unrelated NAM keeps its loaded model instead of
/// reloading from disk on every trial (#791 perf). Returns the samples + the
/// nodes to thread into the next call.
pub(crate) fn render_reusing(
    chain: &Chain,
    sample_rate: f32,
    input: &[[f32; 2]],
    block_size: usize,
    tail_frames: usize,
    base: Option<Vec<BlockRuntimeNode>>,
) -> Result<(Vec<[f32; 2]>, Vec<BlockRuntimeNode>)> {
    let (mut nodes, _layout) =
        build_runtime_block_nodes(chain, AudioChannelLayout::Stereo, false, sample_rate, base, None)?;
    let mask: Vec<bool> = chain.blocks.iter().map(|b| b.enabled).collect();
    let out = render_nodes_masked(&mut nodes, input, block_size, tail_frames, &mask);
    Ok((out, nodes))
}

/// Render pre-built `nodes` over `input`, processing only the nodes whose
/// `enabled[i]` is true (falling back to the node's own snapshot flag when the
/// mask is shorter). Reuses the loaded processors — the NAM is NOT reloaded.
///
/// Nodes keep their DSP state across calls; for the Welch-averaged descriptors
/// the sub-100 ms warmup carry-over is negligible against a multi-second window.
pub(crate) fn render_nodes_masked(
    nodes: &mut [BlockRuntimeNode],
    input: &[[f32; 2]],
    block_size: usize,
    tail_frames: usize,
    enabled: &[bool],
) -> Vec<[f32; 2]> {
    let total_frames = input.len() + tail_frames;
    let mut output: Vec<[f32; 2]> = Vec::with_capacity(total_frames);
    let mut chunk_buf: Vec<AudioFrame> = Vec::with_capacity(block_size);
    let mut frame_idx = 0_usize;
    while frame_idx < total_frames {
        let chunk_size = block_size.min(total_frames - frame_idx);
        chunk_buf.clear();
        for i in 0..chunk_size {
            let g = frame_idx + i;
            let pair = if g < input.len() {
                input[g]
            } else {
                [0.0_f32, 0.0_f32]
            };
            chunk_buf.push(AudioFrame::Stereo(pair));
        }
        for (i, node) in nodes.iter_mut().enumerate() {
            let on = enabled.get(i).copied().unwrap_or(node.block_snapshot.enabled);
            if !on {
                continue;
            }
            apply_block_offline(node, &mut chunk_buf);
        }
        for frame in chunk_buf.iter() {
            let pair = match frame {
                AudioFrame::Stereo([l, r]) => [*l, *r],
                AudioFrame::Mono(s) => [*s, *s],
            };
            output.push(pair);
        }
        frame_idx += chunk_size;
    }
    output
}

fn collect_faulted_blocks(nodes: &[BlockRuntimeNode]) -> Vec<FaultedBlock> {
    nodes
        .iter()
        .filter_map(|node| {
            let reason = node.fault_reason.as_ref()?;
            let (effect_type, model) = match node.block_snapshot.model_ref() {
                Some(m) => (m.effect_type.to_string(), m.model.to_string()),
                None => (node.block_snapshot.kind.label().to_string(), String::new()),
            };
            Some(FaultedBlock {
                block_id: node.block_id.0.clone(),
                effect_type,
                model,
                error: reason.clone(),
            })
        })
        .collect()
}

fn apply_block_offline(node: &mut BlockRuntimeNode, frames: &mut [AudioFrame]) {
    if node.faulted {
        return;
    }
    match &mut node.processor {
        RuntimeProcessor::Audio(processor) => {
            processor.process_buffer(frames, &mut node.scratch);
        }
        RuntimeProcessor::Select(select) => {
            if let Some(selected) = select.selected_node_mut() {
                apply_block_offline(selected, frames);
            }
        }
        RuntimeProcessor::Bypass => {}
    }
}
