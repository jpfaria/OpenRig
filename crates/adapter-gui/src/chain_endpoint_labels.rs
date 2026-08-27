//! Responsibility: formats the endpoint text a chain row prints.

use project::block::AudioBlockKind;
use project::chain::Chain;

pub(crate) fn chain_endpoint_label(prefix: &str, _channels: &[usize]) -> String {
    prefix.to_string()
}

pub(crate) fn format_channel_list(channels: &[usize]) -> String {
    if channels.is_empty() {
        "-".to_string()
    } else {
        channels
            .iter()
            .map(|channel| (channel + 1).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Map a real chain.blocks index to the UI block index (which excludes hidden first Input and last Output).
pub(crate) fn real_block_index_to_ui(chain: &Chain, real_index: usize) -> Option<usize> {
    let first_input_idx = chain
        .blocks
        .iter()
        .position(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
    let last_output_idx = chain
        .blocks
        .iter()
        .rposition(|b| matches!(&b.kind, AudioBlockKind::Output(_)));
    let mut visible_count = 0;
    for (idx, _) in chain.blocks.iter().enumerate() {
        if Some(idx) == first_input_idx || Some(idx) == last_output_idx {
            continue;
        }
        if idx == real_index {
            return Some(visible_count);
        }
        visible_count += 1;
    }
    None
}
