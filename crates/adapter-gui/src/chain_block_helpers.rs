//! Chain-block bookkeeping for the editing screens: block identity when a
//! chain is cloned, and the UI-position → real-position mapping.
//!
//! Both used to sit in `runtime_lifecycle`, which owns the audio controller
//! and has nothing to do with either. They are pure functions over
//! `project::chain::Chain` — no runtime, no Slint, no I/O.

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;

/// Reassign a fresh id to every block of `chain`, recursing into `Select`
/// options. Called when a chain is cloned so two live chains never share a
/// block id — a shared id makes a per-block runtime lookup ambiguous.
pub(crate) fn assign_new_block_ids(chain: &mut Chain) {
    for block in &mut chain.blocks {
        assign_new_block_ids_recursive(block, &chain.id);
    }
}

fn assign_new_block_ids_recursive(block: &mut AudioBlock, chain_id: &ChainId) {
    block.id = BlockId::generate_for_chain(chain_id);
    if let AudioBlockKind::Select(select) = &mut block.kind {
        for option in &mut select.options {
            assign_new_block_ids_recursive(option, chain_id);
        }
    }
}

/// Map a UI block index (which excludes hidden first Input and last Output) to the real chain.blocks index.
pub(crate) fn ui_index_to_real_block_index(chain: &Chain, ui_index: usize) -> usize {
    let first_input_idx = chain
        .blocks
        .iter()
        .position(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
    let last_output_idx = chain
        .blocks
        .iter()
        .rposition(|b| matches!(&b.kind, AudioBlockKind::Output(_)));
    let mut visible_count = 0;
    for (real_idx, _) in chain.blocks.iter().enumerate() {
        if Some(real_idx) == first_input_idx || Some(real_idx) == last_output_idx {
            continue; // hidden
        }
        if visible_count == ui_index {
            return real_idx;
        }
        visible_count += 1;
    }
    // If ui_index is past all visible blocks, return end (before last output)
    last_output_idx.unwrap_or(chain.blocks.len())
}
