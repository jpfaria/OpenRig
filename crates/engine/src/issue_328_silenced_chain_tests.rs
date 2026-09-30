//! #328 — a chain whose checklist leaves it no input or no output has nothing
//! to play (spec §5.3: allowed). It is off: no runtime, no stream, and it
//! claims no input tap another chain wants.

use std::collections::HashMap;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointRef};
use project::project::Project;

use super::{build_runtime_graph, chain_plays};
use crate::input_conflicts::{conflicting_input_channel, input_conflicting_chains};

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn off(endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: "main".into(),
        endpoint: endpoint.into(),
    }
}

fn chain(id: &str, enabled: bool, disabled_endpoints: EndpointDisables) -> Chain {
    Chain {
        mix: Default::default(),
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

fn no_outputs() -> EndpointDisables {
    EndpointDisables {
        outputs: vec![off("out")],
        ..EndpointDisables::default()
    }
}

fn no_inputs() -> EndpointDisables {
    EndpointDisables {
        inputs: vec![off("in")],
        ..EndpointDisables::default()
    }
}

#[test]
fn a_chain_plays_only_when_on_with_an_input_and_an_output() {
    let reg = registry();
    assert!(chain_plays(
        &chain("a", true, EndpointDisables::default()),
        &reg
    ));
    assert!(
        !chain_plays(&chain("b", false, EndpointDisables::default()), &reg),
        "switched off"
    );
    assert!(
        !chain_plays(&chain("c", true, no_outputs()), &reg),
        "#328: every output unchecked"
    );
    assert!(
        !chain_plays(&chain("d", true, no_inputs()), &reg),
        "#328: every input unchecked"
    );
}

#[test]
fn the_graph_leaves_out_a_chain_with_every_output_unchecked() {
    let silenced = chain("rig:input-1", true, no_outputs());
    let mut rates = HashMap::new();
    rates.insert(silenced.id.clone(), 48_000.0_f32);
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![silenced],
        midi: None,
    };
    let graph = build_runtime_graph(&project, &rates, &HashMap::new(), &registry())
        .expect("the graph builds");
    assert!(
        graph.chains.is_empty(),
        "#328: a chain with nothing to play gets no runtime — got {} runtime(s)",
        graph.chains.len()
    );
}

#[test]
fn a_silenced_chain_claims_no_input_tap() {
    let silenced = chain("rig:input-1", true, no_outputs());
    let player = chain("rig:input-2", true, EndpointDisables::default());
    let skipped = input_conflicting_chains([&silenced, &player], &registry());
    assert!(
        skipped.is_empty(),
        "#328: the chain with every output unchecked plays nothing — it must not keep input ch 0 from the chain that does, got {skipped:?}"
    );
    assert!(
        conflicting_input_channel(&player, [&silenced, &player], &registry()).is_none(),
        "#328: enabling the player must not be refused because of a silenced chain"
    );
}
