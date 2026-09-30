//! #328 — Linux/JACK-direct runs ONE client for a chain's whole runtime and
//! gives every output route of that runtime its own JACK output ports. It
//! learns how many routes to serve from `output_route_count`.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

use crate::runtime::{build_chain_runtime_state, DEFAULT_ELASTIC_TARGET};

fn stereo_out(name: &str, channels: [usize; 2]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels: channels.to_vec(),
    }
}

fn chain_with_outputs(outputs: Vec<IoEndpoint>) -> (Chain, Vec<IoBinding>) {
    let registry = vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs,
    }];
    let chain = Chain {
        id: ChainId("issue-328-routes".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
    };
    (chain, registry)
}

#[test]
fn a_runtime_reports_one_route_per_output() {
    let (chain, registry) = chain_with_outputs(vec![
        stereo_out("out-a", [0, 1]),
        stereo_out("out-b", [2, 3]),
    ]);
    let runtime =
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET; 2], &registry)
            .expect("the chain builds");
    assert_eq!(
        runtime.output_route_count(),
        2,
        "#328: JACK-direct must see both output routes of the runtime"
    );
}

#[test]
fn a_single_output_runtime_reports_one_route() {
    let (chain, registry) = chain_with_outputs(vec![stereo_out("out", [0, 1])]);
    let runtime =
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET; 2], &registry)
            .expect("the chain builds");
    assert_eq!(runtime.output_route_count(), 1);
}
