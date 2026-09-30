//! #328 (spec §3) — graph gestures reach the project through the bus.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::chain_graph_adapter::{CARD_HALF, COLUMN_SPACING};
use crate::chain_graph_fixtures_tests::{
    chain, chain_in, core, mix_chain, port_in, registry, rows, session_with, split,
};
use crate::chain_graph_ids::{MIXER_NODE_ID, SPLIT_NODE_ID};
use crate::graph_view_model::AnchorSlot;
use crate::state::ProjectSession;
use project::block::{AudioBlock, AudioBlockKind, SplitEnd};
use slint::{Model, VecModel};

fn target(rows: &Rc<VecModel<ProjectChainItem>>) -> RowsTarget<'_> {
    RowsTarget {
        model: rows,
        inputs: &[],
        outputs: &[],
    }
}

fn ids(list: &[AudioBlock]) -> Vec<String> {
    list.iter().map(|b| b.id.0.clone()).collect()
}

fn lanes(
    session: &Rc<RefCell<Option<ProjectSession>>>,
) -> Option<(Vec<AudioBlock>, Vec<AudioBlock>)> {
    chain_in(session, 0)
        .blocks
        .iter()
        .find_map(|b| match &b.kind {
            AudioBlockKind::Split(s) => Some((s.a.clone(), s.b.clone())),
            _ => None,
        })
}

/// The anchor id Part 5 gives the wire before blueprint `index` of `lane`.
/// In both chains below the split is top-level block 1, so its stage is 2.
fn lane_anchor(lane: usize, index: usize) -> String {
    AnchorSlot::Lane {
        stage: 2,
        lane,
        index,
    }
    .anchor_id()
}

#[test]
fn bypass_on_a_path_card_toggles_that_block() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    toggle_node(&session, 0, "b1", &target(&rows)).expect("toggle");
    assert!(!lanes(&session).unwrap().1[0].enabled);
    assert!(chain_in(&session, 0).blocks[0].enabled, "pre untouched");
}

#[test]
fn an_io_node_has_nothing_to_bypass() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    assert_eq!(
        toggle_node(
            &session,
            0,
            crate::chain_graph_ids::INPUT_NODE_ID,
            &target(&rows)
        ),
        Err(GestureError::NotApplicable)
    );
}

#[test]
fn removing_a_path_block_removes_it_from_its_lane() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    assert_eq!(
        remove_node(&session, 0, "a2", &target(&rows)),
        Ok(RemoveOutcome::Removed)
    );
    assert_eq!(ids(&lanes(&session).unwrap().0), vec!["a1"]);
}

#[test]
fn removing_a_split_with_blocks_in_path_b_asks_first() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    let outcome = remove_node(&session, 0, SPLIT_NODE_ID, &target(&rows)).expect("decided");
    assert!(
        matches!(outcome, RemoveOutcome::ConfirmSplit { .. }),
        "got {outcome:?}"
    );
    assert!(
        lanes(&session).is_some(),
        "nothing removed before the confirmation"
    );
}

#[test]
fn removing_a_split_with_an_empty_path_b_needs_no_confirmation() {
    let c = chain(vec![
        core("pre"),
        split("sp", SplitEnd::Mix, vec![core("a1")], vec![]),
        core("post"),
    ]);
    let (session, rows) = (session_with(vec![c]), rows());
    assert_eq!(
        remove_node(&session, 0, MIXER_NODE_ID, &target(&rows)),
        Ok(RemoveOutcome::Removed)
    );
    // spec §3: path A's blocks take the split's place.
    assert_eq!(
        ids(&chain_in(&session, 0).blocks),
        vec!["pre", "a1", "post"]
    );
}

#[test]
fn confirming_removes_the_split_and_keeps_path_a() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    remove_split(&session, 0, &target(&rows)).expect("remove");
    assert_eq!(
        ids(&chain_in(&session, 0).blocks),
        vec!["pre", "a1", "a2", "post"]
    );
}

#[test]
fn dragging_a_card_across_lanes_moves_it_into_the_other_path() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    // b1 → mixer: the end of lane B.
    drop_node(&session, 0, "a1", &lane_anchor(1, 1), &target(&rows)).expect("move");
    let (a, b) = lanes(&session).unwrap();
    assert_eq!(
        (ids(&a), ids(&b)),
        (
            vec!["a2".to_string()],
            vec!["b1".to_string(), "a1".to_string()]
        )
    );
}

fn a1_x(rows: &Rc<VecModel<ProjectChainItem>>) -> f32 {
    rows.row_data(0)
        .unwrap()
        .graph_nodes
        .iter()
        .find(|n| n.id.as_str() == "a1")
        .unwrap()
        .layout_x
}

#[test]
fn a_no_op_drop_snaps_the_card_back() {
    let (session, rows) = (session_with(vec![mix_chain()]), rows());
    crate::project_view::replace_project_chains(
        &rows,
        &session.borrow().as_ref().unwrap().project.borrow(),
        &[],
        &[],
        &registry(),
    );
    crate::chain_graph_drag::move_node(&rows, 0, "a1", 999.0, 999.0);
    // split → a1: a1's own slot.
    let own_slot = lane_anchor(0, 0);
    assert_eq!(
        drop_node(&session, 0, "a1", &own_slot, &target(&rows)),
        Err(GestureError::NoMove)
    );
    assert_eq!(
        a1_x(&rows),
        CARD_HALF + 3.0 * COLUMN_SPACING,
        "the card is back in its slot"
    );
}

#[test]
fn a_refused_drop_snaps_the_card_back() {
    // spec §1.1: an Input port may not live in a path, so Part 2 refuses the move.
    let c = chain(vec![
        port_in("port", "main", "In 1"),
        split("sp", SplitEnd::Mix, vec![core("a1")], vec![]),
    ]);
    let (session, rows) = (session_with(vec![c]), rows());
    crate::project_view::replace_project_chains(
        &rows,
        &session.borrow().as_ref().unwrap().project.borrow(),
        &[],
        &[],
        &registry(),
    );
    crate::chain_graph_drag::move_node(&rows, 0, "port", 999.0, 999.0);
    // split → mixer of the empty lane B.
    assert!(matches!(
        drop_node(&session, 0, "port", &lane_anchor(1, 0), &target(&rows)),
        Err(GestureError::Failed(_))
    ));
    let x = rows
        .row_data(0)
        .unwrap()
        .graph_nodes
        .iter()
        .find(|n| n.id.as_str() == "port")
        .unwrap()
        .layout_x;
    assert_eq!(
        x,
        CARD_HALF + COLUMN_SPACING,
        "the port is back in its slot"
    );
}
