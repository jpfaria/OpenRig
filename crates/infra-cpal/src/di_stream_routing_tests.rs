//! #717 — arming a DI stream must APPEND the DI runtime to the chain's output
//! stream slot list (matched by rate), so the backend mixes it onto that device
//! without a rebuild; disarming must remove it. This exercises the routing logic
//! directly against a hand-built `ActiveChainRuntime` (no real cpal streams).

#![cfg(test)]

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::DiPcm;
use project::chain::Chain;

use super::ProjectRuntimeController;
use crate::LiveRuntimeSlot;

fn chain_and_registry() -> (Chain, Vec<IoBinding>) {
    let chain = Chain {
        id: ChainId("route".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
    };
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    (chain, registry)
}

#[test]
fn arm_routes_di_onto_the_output_slot_list_and_disarm_removes_it() {
    let (chain, registry) = chain_and_registry();
    let guitar = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256], &registry).expect("guitar runtime"),
    );
    let mut chains = HashMap::new();
    chains.insert((chain.id.clone(), 0usize), guitar.clone());
    let mut controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    controller.set_io_bindings(registry);

    // Stand in for the chain's live output stream: one slot list (the guitar) at
    // the device rate, the shape `build_chain_streams` produces.
    let out_list: crate::OutputSlotList = Arc::new(ArcSwap::from_pointee(vec![
        LiveRuntimeSlot::new(guitar),
    ]));
    controller.active_chains.insert(
        chain.id.clone(),
        crate::active_runtime::ActiveChainRuntime {
            stream_signature: crate::resolved::ChainStreamSignature {
                inputs: vec![],
                outputs: vec![],
            },
            _input_streams: vec![],
            _output_streams: vec![],
            output_slot_lists: vec![(48_000.0, out_list.clone())],
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _jack_client: None,
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _dsp_worker: None,
        },
    );

    assert_eq!(out_list.load().len(), 1, "precondition: guitar-only output");

    let pcm = Arc::new(DiPcm::new(vec![0.5; 4800], 48_000, 1));
    controller.arm_di_stream(&chain, pcm).expect("arm DI");
    assert_eq!(
        out_list.load().len(),
        2,
        "#717: arming must append the DI runtime to the output's slot list so the \
         backend mixes it onto that device"
    );

    controller.disarm_di_stream(&chain.id);
    assert_eq!(
        out_list.load().len(),
        1,
        "#717: disarming must remove the DI runtime from the output's slot list"
    );
}
