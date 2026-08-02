//! Issue #85 — the owner cannot place an `Output` (or `Input`) block on a chain:
//! selecting the chain and trying to add one does nothing.
//!
//! The Command path is proven fine (driving the live app over MCP,
//! `InsertPrebuiltBlock` with an `Output` kind lands the block in the chain), so
//! the intended behaviour asserted here is the one the UI owes: the block-type
//! picker the chain editor renders must OFFER the I/O types. A type that is not
//! in the picker can never be picked — which is exactly "nothing happens".

use adapter_gui::project_view::block_type_picker_items;

#[test]
fn block_type_picker_offers_output() {
    let items = block_type_picker_items("electric_guitar");
    let has_output = items
        .iter()
        .any(|item| item.effect_type.as_str() == "output");
    assert!(
        has_output,
        "the block-type picker does not offer OUTPUT, so a mid-chain output can never be \
         placed from the UI. Items: {:?}",
        items.iter().map(|i| &i.effect_type).collect::<Vec<_>>()
    );
}

#[test]
fn block_type_picker_offers_input() {
    let items = block_type_picker_items("electric_guitar");
    let has_input = items
        .iter()
        .any(|item| item.effect_type.as_str() == "input");
    assert!(
        has_input,
        "the block-type picker does not offer INPUT. Items: {:?}",
        items.iter().map(|i| &i.effect_type).collect::<Vec<_>>()
    );
}
