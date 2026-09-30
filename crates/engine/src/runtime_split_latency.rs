//! Responsibility: measures how many samples of delay a path of runtime nodes adds.

use crate::runtime_audio_frame::AudioProcessor;
use crate::runtime_state::{BlockRuntimeNode, RuntimeProcessor};

/// Processing latency of one built processor. A dual-mono pair runs one
/// model per channel; the larger report is taken.
pub(crate) fn processor_latency(processor: &AudioProcessor) -> usize {
    match processor {
        AudioProcessor::Mono(processor) => processor.latency_samples(),
        AudioProcessor::DualMono { left, right } => {
            left.latency_samples().max(right.latency_samples())
        }
        AudioProcessor::Stereo(processor) | AudioProcessor::StereoFromMono(processor) => {
            processor.latency_samples()
        }
    }
}

/// Delay the node adds while it processes: nothing when it is switched off.
pub(crate) fn node_latency(node: &BlockRuntimeNode) -> usize {
    if node.block_snapshot.enabled {
        node_latency_ceiling(node)
    } else {
        0
    }
}

/// Delay the node adds once enabled — the room a path keeps for a toggle
/// that switches it back on. A faulted node never processes.
pub(crate) fn node_latency_ceiling(node: &BlockRuntimeNode) -> usize {
    if node.faulted {
        return 0;
    }
    match &node.processor {
        RuntimeProcessor::Audio(processor) => processor_latency(processor),
        RuntimeProcessor::Select(select) => select
            .options
            .iter()
            .find(|option| option.block_id == select.selected_block_id)
            .map(node_latency)
            .unwrap_or(0),
        RuntimeProcessor::Split(split) => path_latency(&split.a).max(path_latency(&split.b)),
        RuntimeProcessor::Bypass => 0,
    }
}

pub(crate) fn path_latency(nodes: &[BlockRuntimeNode]) -> usize {
    nodes.iter().map(node_latency).sum()
}

pub(crate) fn path_latency_ceiling(nodes: &[BlockRuntimeNode]) -> usize {
    nodes.iter().map(node_latency_ceiling).sum()
}

#[cfg(test)]
#[path = "runtime_split_latency_tests.rs"]
mod tests;
