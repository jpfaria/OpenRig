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

/// Map a UI block position to the real `chain.blocks` index.
pub(crate) fn ui_index_to_real_block_index(chain: &Chain, ui_index: usize) -> usize {
    // Model A (#716): the chain's head input and tail output are NOT blocks —
    // they are materialized from its E/S bindings and drawn as fixed chips. So
    // every entry in `chain.blocks` is a block the user placed, including a mid
    // `Input`/`Output`/`Insert` port, and the UI row list shows all of them:
    // the mapping is the identity.
    //
    // #85: this used to skip the first `Input` and the LAST `Output`, a leftover
    // from when those were real head/tail blocks. With no head/tail blocks left,
    // a single mid `Output` IS the last output — so the row the user had just
    // added was swallowed, and every index derived from a UI row (select, move,
    // insert-before) pointed one block off.
    ui_index.min(chain.blocks.len())
}
