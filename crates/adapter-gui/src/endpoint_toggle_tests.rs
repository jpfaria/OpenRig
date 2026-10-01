//! #328 (spec §5.3) — a checklist row becomes exactly one
//! `SetChainEndpointEnabled` for that node and that endpoint, then a resync.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, recording_session, rows, y_chain};
use crate::chain_graph_ids::{leaf_output_node_id, INPUT_NODE_ID, OUTPUT_NODE_ID};
use crate::graph_gesture_actions::{GestureError, RowsTarget};
use application::command::{ChainCommand, Command};
use domain::ids::BlockId;
use project::block::PathRef;
use project::endpoint_disables::EndpointNode;
use slint::VecModel;
use std::rc::Rc;

fn target(rows: &Rc<VecModel<crate::ProjectChainItem>>) -> RowsTarget<'_> {
    RowsTarget {
        model: rows,
        inputs: &[],
        outputs: &[],
    }
}

#[test]
fn unchecking_an_input_names_that_endpoint_then_resyncs() {
    let (session, recorder) = recording_session(vec![chain(vec![core("amp")])]);
    let rows = rows();
    set_endpoint_enabled(&session, 0, INPUT_NODE_ID, 1, false, &target(&rows)).expect("toggle");
    let seen = recorder.seen.borrow();
    assert!(
        matches!(&seen[0], Command::Chain(ChainCommand::SetChainEndpointEnabled { node: EndpointNode::Input, io, endpoint, enabled: false, .. }) if io == "main" && endpoint == "In 2"),
        "got {:?}",
        seen[0]
    );
    assert!(
        matches!(
            &seen[1],
            Command::Chain(ChainCommand::SyncChainRuntime { .. })
        ),
        "#614 resync"
    );
}

#[test]
fn unchecking_the_last_output_is_allowed() {
    let mut only_main = chain(vec![]);
    only_main.io_binding_ids = vec!["main".into()];
    let (session, recorder) = recording_session(vec![only_main]);
    let rows = rows();
    set_endpoint_enabled(&session, 0, OUTPUT_NODE_ID, 0, false, &target(&rows))
        .expect("spec §5.3: allowed");
    assert!(matches!(
        &recorder.seen.borrow()[0],
        Command::Chain(ChainCommand::SetChainEndpointEnabled {
            node: EndpointNode::Output,
            enabled: false,
            ..
        })
    ));
}

#[test]
fn a_lane_output_node_addresses_its_own_node() {
    let (session, recorder) = recording_session(vec![y_chain()]);
    let rows = rows();
    let leaf = PathRef {
        split: BlockId("sp".into()),
        path: 1,
    };
    let node_id = leaf_output_node_id(&leaf);
    set_endpoint_enabled(&session, 0, &node_id, 1, false, &target(&rows)).expect("toggle");
    assert!(matches!(
        &recorder.seen.borrow()[0],
        Command::Chain(ChainCommand::SetChainEndpointEnabled { node: EndpointNode::PathOutput(r), io, .. }) if io == "aux" && *r == leaf
    ));
}

#[test]
fn a_block_node_has_no_endpoints() {
    let (session, _) = recording_session(vec![chain(vec![core("amp")])]);
    let rows = rows();
    assert_eq!(
        set_endpoint_enabled(&session, 0, "amp", 0, false, &target(&rows)),
        Err(GestureError::NotApplicable)
    );
}
