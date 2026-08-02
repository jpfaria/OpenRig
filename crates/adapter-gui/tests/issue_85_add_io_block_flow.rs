//! Issue #85 — picking an `Output` (or `Input`) block type on a chain does
//! nothing: no block is inserted, no editor opens.
//!
//! Already ruled out by test: the type picker DOES offer both types
//! (`issue_85_io_block_type_picker`), and the detached editor DOES build for
//! both (`issue_815_add_block_tabs_tests::adding_an_output_block_opens_its_editor`).
//! So the break is between them — in what the add flow calls to MATERIALIZE the
//! block once a type/model is chosen.
//!
//! Intended behaviour asserted here: the block factory the add path goes through
//! must be able to build an I/O block. A type the factory refuses can never be
//! added, which is exactly "I click and nothing happens".

use domain::ids::BlockId;
use project::block::AudioBlockKind;

#[test]
fn block_factory_builds_an_output_block() {
    let built = application::block_factory::build_default_block(
        BlockId("issue85:out".into()),
        "output",
        "standard",
    );
    let block = built.unwrap_or_else(|e| {
        panic!(
            "block_factory refused to build an OUTPUT block, so picking the type on a chain \
             materializes nothing — the user clicks and nothing happens (#85): {e}"
        )
    });
    assert!(
        matches!(block.kind, AudioBlockKind::Output(_)),
        "the factory built a {} block for the OUTPUT type",
        block.kind.label()
    );
}

#[test]
fn block_factory_builds_an_input_block() {
    let built = application::block_factory::build_default_block(
        BlockId("issue85:in".into()),
        "input",
        "standard",
    );
    let block = built
        .unwrap_or_else(|e| panic!("block_factory refused to build an INPUT block (#85): {e}"));
    assert!(
        matches!(block.kind, AudioBlockKind::Input(_)),
        "the factory built a {} block for the INPUT type",
        block.kind.label()
    );
}
