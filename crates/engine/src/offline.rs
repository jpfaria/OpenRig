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
) -> Result<Vec<[f32; 2]>> {
    if block_size == 0 {
        anyhow::bail!("block_size must be > 0");
    }
    let (mut nodes, _output_layout) =
        build_runtime_block_nodes(chain, AudioChannelLayout::Stereo, sample_rate, None, None)?;

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
    Ok(output)
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
