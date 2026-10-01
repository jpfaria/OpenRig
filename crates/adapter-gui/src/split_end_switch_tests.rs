//! #328 (orchestrator decision 9) — the split editor's Mix / Y switch is a
//! `SetSplitEnd` on the bus; a refused switch reports the command's error.

use super::*;
use crate::chain_graph_fixtures_tests::{chain_in, mix_chain, rows, session_with, y_chain};
use crate::graph_gesture_actions::{GestureError, RowsTarget};
use project::block::{AudioBlockKind, SplitEnd};

fn end_of(session: &Rc<RefCell<Option<ProjectSession>>>) -> SplitEnd {
    let AudioBlockKind::Split(split) = &chain_in(session, 0).blocks[1].kind else {
        panic!("block 1 is the split")
    };
    split.end
}

fn switch(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    end: SplitEnd,
) -> Result<(), GestureError> {
    let model = rows();
    set_split_end(
        session,
        0,
        &BlockId("sp".into()),
        end,
        &RowsTarget {
            model: &model,
            inputs: &[],
            outputs: &[],
        },
    )
}

#[test]
fn a_y_switches_to_a_mix() {
    let session = session_with(vec![y_chain()]);
    switch(&session, SplitEnd::Mix).expect("Y → Mix is allowed");
    assert_eq!(end_of(&session), SplitEnd::Mix);
}

#[test]
fn a_mix_with_a_block_after_it_refuses_the_y() {
    let session = session_with(vec![mix_chain()]);
    let result = switch(&session, SplitEnd::Y);
    assert!(
        matches!(result, Err(GestureError::Failed(_))),
        "the command's refusal comes back: {result:?}"
    );
    assert_eq!(end_of(&session), SplitEnd::Mix);
}

#[test]
fn a_chain_without_a_split_has_nothing_to_switch() {
    let session = session_with(vec![crate::chain_graph_fixtures_tests::chain(vec![])]);
    assert_eq!(
        switch(&session, SplitEnd::Y),
        Err(GestureError::NotApplicable)
    );
}
