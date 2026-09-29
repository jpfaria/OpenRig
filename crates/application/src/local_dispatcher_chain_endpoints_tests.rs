//! #328 — the endpoint checklist of the chain graph's I/O nodes: recorded per
//! node, carried into the rig, and kept by the commands that rewrite a chain.

use project::endpoint_disables::{EndpointNode, EndpointRef};
use project::rig::RigScene;
use serde_json::json;

use crate::command::RigNavKind;
use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

fn endpoint(io: &str, name: &str) -> EndpointRef {
    EndpointRef {
        io: io.to_string(),
        endpoint: name.to_string(),
    }
}

fn set_enabled(dispatcher: &LocalDispatcher, chain: &str, node: &str, name: &str, enabled: bool) {
    dispatch_json(
        dispatcher,
        "SetChainEndpointEnabled",
        json!({ "chain": chain, "node": node, "io": "io-main", "endpoint": name, "enabled": enabled }),
    )
    .expect("SetChainEndpointEnabled");
}

#[test]
fn unchecking_an_input_records_it_on_that_node_only() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatch_json(
        &dispatcher,
        "SetChainEndpointEnabled",
        json!({ "chain": CHAIN, "node": "input", "io": "io-main", "endpoint": "In 1", "enabled": false }),
    )
    .expect("uncheck In 1 on the input node");

    let disables = project.borrow().chains[0].disabled_endpoints.clone();
    assert_eq!(disables.inputs, vec![endpoint("io-main", "In 1")]);
    assert!(!disables.is_enabled(EndpointNode::Input, &endpoint("io-main", "In 1")));
    assert!(
        disables.is_enabled(EndpointNode::Output, &endpoint("io-main", "In 1")),
        "another node is untouched"
    );
    assert!(
        events
            .iter()
            .any(|e| e.chain() == Some(&ChainId(CHAIN.into()))),
        "the drain must re-sync exactly this chain: {events:?}"
    );
}

#[test]
fn checking_it_again_clears_it_and_a_repeated_uncheck_is_recorded_once() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    set_enabled(&dispatcher, CHAIN, "input", "In 1", false);
    set_enabled(&dispatcher, CHAIN, "input", "In 1", false);
    assert_eq!(
        project.borrow().chains[0].disabled_endpoints.inputs,
        vec![endpoint("io-main", "In 1")],
        "unchecked twice, listed once"
    );

    set_enabled(&dispatcher, CHAIN, "input", "In 1", true);
    assert!(
        project.borrow().chains[0].disabled_endpoints.is_empty(),
        "checked again: nothing is disabled"
    );
}

#[test]
fn a_path_b_output_lands_on_the_path_b_list_only() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    set_enabled(&dispatcher, CHAIN, "path_b_output", "Out 3", false);

    let disables = project.borrow().chains[0].disabled_endpoints.clone();
    assert_eq!(disables.path_b_outputs, vec![endpoint("io-main", "Out 3")]);
    assert!(disables.inputs.is_empty());
    assert!(disables.outputs.is_empty());
    assert!(disables.path_a_outputs.is_empty());
}

#[test]
fn an_unknown_chain_is_refused() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatch_json(
        &dispatcher,
        "SetChainEndpointEnabled",
        json!({ "chain": "nope", "node": "input", "io": "io-main", "endpoint": "In 1", "enabled": false }),
    );

    let err = result.expect_err("no chain 'nope'");
    assert!(err.to_string().contains("chain not found"), "{err}");
}

#[test]
fn the_checklist_is_captured_into_the_rig_input() {
    let (rig, _project, dispatcher) = rig_session_from(rig_with_presets(vec![(
        "p1",
        vec![make_core_block("A", true)],
    )]));

    set_enabled(&dispatcher, RIG_CHAIN, "output", "Out 1", false);
    dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture");

    assert_eq!(
        rig.borrow().inputs["in"].disabled_endpoints.outputs,
        vec![endpoint("io-main", "Out 1")],
        "project.openrig persists the checklist on the input"
    );
}

#[test]
fn the_checklist_survives_a_scene_switch() {
    let mut rig = rig_with_presets(vec![("p1", vec![make_core_block("A", true)])]);
    rig.presets
        .get_mut("p1")
        .expect("p1")
        .scenes
        .insert(2, RigScene::default());
    let (_rig, project, dispatcher) = rig_session_from(rig);

    set_enabled(&dispatcher, RIG_CHAIN, "output", "Out 1", false);
    dispatcher
        .dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
            chain: ChainId(RIG_CHAIN.into()),
            kind: RigNavKind::Scene(2),
        }))
        .expect("switch to scene 2");

    assert_eq!(
        project.borrow().chains[0].disabled_endpoints.outputs,
        vec![endpoint("io-main", "Out 1")],
        "a scene switch re-projects the chain; the checklist must come back with it"
    );
}

#[test]
fn the_chain_editor_save_keeps_the_checklist() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    set_enabled(&dispatcher, CHAIN, "input", "In 1", false);

    let mut edited = project.borrow().chains[0].clone();
    edited.description = Some("Renamed".into());
    edited.disabled_endpoints = Default::default();
    dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain: edited }))
        .expect("SaveChain (rename)");

    let chain = project.borrow().chains[0].clone();
    assert_eq!(chain.description.as_deref(), Some("Renamed"));
    assert_eq!(
        chain.disabled_endpoints.inputs,
        vec![endpoint("io-main", "In 1")],
        "the editor's Save carries no checklist and must not reset it"
    );
}
