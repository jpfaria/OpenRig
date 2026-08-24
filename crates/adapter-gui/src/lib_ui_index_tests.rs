//! `ui_index_to_real_block_index` tests (issue #127 split from lib_tests.rs).
//!
//! One subject: the mapping between a chain-editor ROW and the index of the
//! block it stands for. Model A (#716) + #85 made that mapping the identity —
//! nothing is hidden anymore — and these cases pin it across every chain shape
//! that used to be special: mid inputs, mid outputs, a chain of only I/O
//! blocks, and asking past the last row (which means "append").
//!
//! Chain fixtures come from `super::tests`, which the whole `lib_*_tests`
//! family shares.

use super::tests::{effect_kind, input_kind, output_kind, test_chain};
use crate::chain_block_helpers::ui_index_to_real_block_index;

/// Model A (#716) + #85: nothing is hidden anymore, so the UI row index IS the
/// block index. These cases used to encode the pre-#716 shape, where the chain
/// carried its own head `Input` and tail `Output` blocks and the row list
/// skipped them; a bound chain no longer persists those (they are dropped on
/// load), and any I/O block that remains is a mid port the user placed.
#[test]
fn ui_index_maps_every_block_of_the_chain() {
    let chain = test_chain(vec![
        effect_kind("dynamics"),
        effect_kind("preamp"),
        effect_kind("delay"),
    ]);
    assert_eq!(ui_index_to_real_block_index(&chain, 0), 0);
    assert_eq!(ui_index_to_real_block_index(&chain, 1), 1);
    assert_eq!(ui_index_to_real_block_index(&chain, 2), 2);
}

#[test]
fn ui_index_past_end_appends_at_the_chain_end() {
    let chain = test_chain(vec![effect_kind("delay")]);
    // Asking past the last row means "append": the end of the block list.
    assert_eq!(ui_index_to_real_block_index(&chain, 5), 1);
}

#[test]
fn ui_index_with_a_mid_input_between_effects() {
    // [Comp, Input_mid, Delay] — the mid input is a row like any other.
    let chain = test_chain(vec![
        effect_kind("dynamics"),
        input_kind(),
        effect_kind("delay"),
    ]);
    assert_eq!(ui_index_to_real_block_index(&chain, 0), 0);
    assert_eq!(ui_index_to_real_block_index(&chain, 1), 1);
    assert_eq!(ui_index_to_real_block_index(&chain, 2), 2);
}

#[test]
fn ui_index_with_two_mid_outputs() {
    // Two mid outputs at different points — BOTH are rows (#85).
    let chain = test_chain(vec![
        effect_kind("dynamics"),
        output_kind(),
        effect_kind("delay"),
        output_kind(),
    ]);
    assert_eq!(ui_index_to_real_block_index(&chain, 1), 1);
    assert_eq!(ui_index_to_real_block_index(&chain, 3), 3);
}

/// #85 (model A, #716): the chain's head input and tail output are NOT blocks
/// anymore — they are materialized from the chain's E/S bindings. So the only
/// I/O block a chain carries today is a MID port the user placed on purpose, and
/// it must be visible. The legacy "hide the first Input and the LAST Output"
/// rule swallows it: a single mid Output IS the last output, so the row the user
/// just added never appears (owner: "não vi nenhum output").
#[test]
fn ui_index_shows_a_mid_output_in_a_model_a_chain() {
    // [Comp, Output_mid, Delay] — no head/tail I/O blocks (model A).
    // UI must see: [Comp(0), Output_mid(1), Delay(2)].
    let chain = test_chain(vec![
        effect_kind("dynamics"),
        output_kind(),
        effect_kind("delay"),
    ]);
    assert_eq!(ui_index_to_real_block_index(&chain, 0), 0, "Comp");
    assert_eq!(
        ui_index_to_real_block_index(&chain, 1),
        1,
        "#85: the mid Output must be a visible row — the user placed it on \
         purpose and it is not the chain's tail output (that comes from the E/S)"
    );
    assert_eq!(ui_index_to_real_block_index(&chain, 2), 2, "Delay");
}

#[test]
fn ui_index_with_no_io_blocks() {
    // [Comp, Delay] — no I/O blocks at all
    let chain = test_chain(vec![effect_kind("dynamics"), effect_kind("delay")]);
    assert_eq!(ui_index_to_real_block_index(&chain, 0), 0);
    assert_eq!(ui_index_to_real_block_index(&chain, 1), 1);
}

#[test]
fn ui_index_with_only_io_blocks() {
    // [Input_mid, Output_mid] — a chain whose only blocks are ports the user
    // placed. Both are rows, so the mapping stays the identity (#85).
    let chain = test_chain(vec![input_kind(), output_kind()]);
    assert_eq!(ui_index_to_real_block_index(&chain, 0), 0);
    assert_eq!(ui_index_to_real_block_index(&chain, 1), 1);
}
