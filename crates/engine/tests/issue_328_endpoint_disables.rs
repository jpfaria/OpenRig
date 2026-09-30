//! #328 — the endpoint checklists reach the engine (spec §1.3): the projected
//! chain carries its input's checklist, and the rig and chain conflict
//! detectors agree on it.

use std::collections::BTreeMap;

use engine::rig_runtime::rig_to_chains;
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};
use project::rig::{RigInput, RigPreset, RigProject};

fn unchecked_input(endpoint: &str) -> EndpointDisables {
    let mut disables = EndpointDisables::default();
    disables.set_enabled(
        EndpointNode::Input,
        EndpointRef {
            io: "shared".into(),
            endpoint: endpoint.into(),
        },
        false,
    );
    disables
}

fn input(disabled_endpoints: EndpointDisables) -> RigInput {
    RigInput {
        mix: Default::default(),
        label: None,
        bank: BTreeMap::from([(1, "p".to_string())]),
        active_preset: 1,
        active_scene: 1,
        routing: Vec::new(),
        instrument: "electric_guitar".into(),
        io: String::new(),
        endpoint: String::new(),
        io_binding_ids: vec!["shared".into()],
        loopers: Vec::new(),
        disabled_endpoints,
    }
}

fn rig(g1: EndpointDisables, g2: EndpointDisables) -> RigProject {
    RigProject {
        name: None,
        inputs: BTreeMap::from([("g1".to_string(), input(g1)), ("g2".to_string(), input(g2))]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([(
            "p".to_string(),
            RigPreset::from_legacy_blocks(Vec::new(), 100.0),
        )]),
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn the_projected_chain_carries_its_inputs_unchecked_endpoints() {
    let chains = rig_to_chains(&rig(unchecked_input("in 2"), EndpointDisables::default()));
    let g1 = chains
        .iter()
        .find(|c| c.id.0 == "rig:g1")
        .expect("g1 is projected");
    assert_eq!(
        g1.disabled_endpoints,
        unchecked_input("in 2"),
        "the synthetic chain carries the rig input's checklist"
    );
}

use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

fn ep(name: &str, channels: Vec<usize>, mode: ChannelMode) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("scarlett".into()),
        mode,
        channels,
    }
}

/// One E/S with two guitar inputs and one stereo output.
fn shared_binding() -> IoBinding {
    IoBinding {
        id: "shared".into(),
        name: "SHARED".into(),
        inputs: vec![
            ep("in 1", vec![0], ChannelMode::Mono),
            ep("in 2", vec![1], ChannelMode::Mono),
        ],
        outputs: vec![ep("out", vec![0, 1], ChannelMode::Stereo)],
    }
}

#[test]
fn complementary_unchecked_inputs_share_one_binding_without_a_tap_conflict() {
    use engine::rig_runtime::RigRuntime;
    use engine::runtime_endpoints::input_conflicting_chains;

    // g1 plays in 1 only, g2 plays in 2 only — one E/S, two guitars.
    let r = rig(unchecked_input("in 2"), unchecked_input("in 1"));
    let registry = vec![shared_binding()];

    let chains = rig_to_chains(&r);
    assert_eq!(
        input_conflicting_chains(chains.iter(), &registry),
        Vec::<domain::ids::ChainId>::new(),
        "chain side: an unchecked input claims no tap"
    );

    let rt = RigRuntime::build(r, 48_000.0, registry).expect("the rig builds");
    assert!(
        rt.is_enabled("g1") && rt.is_enabled("g2"),
        "rig side: g2 must not be refused a tap g1 does not hold (#924: the detectors agree)"
    );
}

/// #328 — a rig input whose checklist leaves it no output plays nothing. The
/// rig side must treat it as off, like the chain side (`chain_plays`): no
/// runtime, no tap held, so the input sharing its E/S still comes up (#924:
/// every detector agrees).
#[test]
fn a_silenced_input_holds_no_tap_on_the_rig_side() {
    use engine::rig_runtime::RigRuntime;
    use engine::runtime_endpoints::input_conflicting_chains;

    // g1 has its only output unchecked; g2 plays. Both select the same E/S.
    let every_output_off = EndpointDisables {
        outputs: vec![EndpointRef {
            io: "shared".into(),
            endpoint: "out".into(),
        }],
        ..EndpointDisables::default()
    };
    let r = rig(every_output_off, EndpointDisables::default());
    let registry = vec![shared_binding()];

    assert_eq!(
        input_conflicting_chains(rig_to_chains(&r).iter(), &registry),
        Vec::<domain::ids::ChainId>::new(),
        "chain side: the silenced g1 claims no tap"
    );
    let mut rt = RigRuntime::build(r, 48_000.0, registry).expect("the rig builds");
    assert!(
        !rt.is_enabled("g1"),
        "#328: g1 has every output unchecked — nothing to play, so it is not brought up"
    );
    assert!(
        rt.is_enabled("g2"),
        "#328: rig side: g2 must not be refused a tap the silenced g1 does not play (#924: the detectors agree)"
    );

    rt.disable_input("g2").expect("g2 goes down");
    rt.enable_input("g1")
        .expect("#328: enabling a silenced input is a no-op, not an error");
    assert!(
        !rt.is_enabled("g1"),
        "#328: enable_input leaves a silenced input off — it holds no tap"
    );
    rt.enable_input("g2")
        .expect("#328: g2 comes back up — the silenced g1 holds no tap");
}
