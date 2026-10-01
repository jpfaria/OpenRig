//! #328 — the endpoint checklist of the chain graph's I/O nodes: recorded per
//! node, carried into the rig, and kept by the commands that rewrite a chain.

use domain::ids::BlockId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{PathRef, SplitEnd};
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
    assert!(!disables.is_enabled(&EndpointNode::Input, &endpoint("io-main", "In 1")));
    assert!(
        disables.is_enabled(&EndpointNode::Output, &endpoint("io-main", "In 1")),
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
fn a_y_leaf_output_lands_on_that_leaf_only() {
    let project = project_with(vec![split("y", SplitEnd::Y, vec![], vec![])]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatch_json(
        &dispatcher,
        "SetChainEndpointEnabled",
        json!({
            "chain": CHAIN,
            "node": { "path_output": { "split": "y", "path": 1 } },
            "io": "io-main",
            "endpoint": "Out 3",
            "enabled": false
        }),
    )
    .expect("uncheck Out 3 on leaf B");

    let disables = project.borrow().chains[0].disabled_endpoints.clone();
    let leaf = |path| {
        EndpointNode::PathOutput(PathRef {
            split: BlockId("y".into()),
            path,
        })
    };
    assert!(!disables.is_enabled(&leaf(1), &endpoint("io-main", "Out 3")));
    assert!(
        disables.is_enabled(&leaf(0), &endpoint("io-main", "Out 3")),
        "leaf A is untouched"
    );
    assert!(disables.inputs.is_empty());
    assert!(disables.outputs.is_empty());
}

#[test]
fn a_leaf_that_is_not_a_y_path_is_refused() {
    let project = project_with(vec![split("mix", SplitEnd::Mix, vec![], vec![])]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatch_json(
        &dispatcher,
        "SetChainEndpointEnabled",
        json!({
            "chain": CHAIN,
            "node": { "path_output": { "split": "mix", "path": 0 } },
            "io": "io-main",
            "endpoint": "Out 3",
            "enabled": false
        }),
    );
    assert!(result.is_err(), "a Mix path has no output node");
    assert!(project.borrow().chains[0].disabled_endpoints.is_empty());
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
        "the project file persists the checklist on the input"
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

#[test]
fn configure_chain_keeps_the_checklist() {
    let project = project_with(vec![make_core_block("amp", true)]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    set_enabled(&dispatcher, CHAIN, "output", "Out 1", false);

    let mut edited = project.borrow().chains[0].clone();
    edited.description = Some("Reconfigured".into());
    edited.disabled_endpoints = Default::default();
    dispatcher
        .dispatch(Command::Chain(ChainCommand::ConfigureChain {
            chain: edited,
        }))
        .expect("ConfigureChain");

    let chain = project.borrow().chains[0].clone();
    assert_eq!(chain.description.as_deref(), Some("Reconfigured"));
    assert_eq!(
        chain.disabled_endpoints.outputs,
        vec![endpoint("io-main", "Out 1")],
        "ConfigureChain carries no checklist and must not reset it"
    );
}

/// The E/S registry of the fixtures' `io-main` binding: In 1, In 2 → Out 1.
fn io_main_registry() -> Rc<RefCell<Vec<IoBinding>>> {
    let ep = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    Rc::new(RefCell::new(vec![IoBinding {
        id: "io-main".into(),
        name: "MAIN".into(),
        inputs: vec![ep("In 1", 0), ep("In 2", 1)],
        outputs: vec![ep("Out 1", 0)],
    }]))
}

#[test]
fn the_save_drops_a_ref_to_an_endpoint_the_io_no_longer_offers() {
    let (rig, _project, dispatcher) = rig_session_from(rig_with_presets(vec![(
        "p1",
        vec![make_core_block("A", true)],
    )]));
    dispatcher.attach_io_bindings(io_main_registry());

    set_enabled(&dispatcher, RIG_CHAIN, "input", "In 1", false);
    // "Gone" was removed from the E/S after the user unchecked it.
    set_enabled(&dispatcher, RIG_CHAIN, "input", "Gone", false);
    dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture (the save path)");

    assert_eq!(
        rig.borrow().inputs["in"].disabled_endpoints.inputs,
        vec![endpoint("io-main", "In 1")],
        "spec §1.3: an unknown ref is dropped on the next save; a known one is kept"
    );
}

#[test]
fn without_a_registry_the_save_prunes_nothing() {
    let (rig, _project, dispatcher) = rig_session_from(rig_with_presets(vec![(
        "p1",
        vec![make_core_block("A", true)],
    )]));

    set_enabled(&dispatcher, RIG_CHAIN, "input", "Gone", false);
    dispatcher
        .dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture");

    assert_eq!(
        rig.borrow().inputs["in"].disabled_endpoints.inputs,
        vec![endpoint("io-main", "Gone")],
        "no registry attached: nothing is known, so nothing may be dropped"
    );
}
