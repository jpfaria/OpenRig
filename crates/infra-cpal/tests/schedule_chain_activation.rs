//! Issue #672 — cold activation. `schedule_chain_activation` builds a chain's
//! runtime off the frontend thread (the NAM/IR load — the bulk of the activation
//! freeze) and returns true; the frontend poll then creates the cpal streams
//! (which are `!Send`, so they must stay on the frontend) and installs the chain.
//!
//! It only handles the single-input-group case (one `ChainRuntimeState`); a
//! multi-input chain needs N per-device runtimes and returns false so the caller
//! falls back to the synchronous build. That guard runs before any device probe,
//! so it is testable without hardware.

use std::collections::HashMap;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::RuntimeGraph;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

fn input_endpoint(name: &str, device: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    }
}

#[test]
fn returns_false_for_a_multi_input_chain() {
    // Two distinct input devices => two per-device runtimes; the single-runtime
    // off-thread path does not apply, so it must defer to the synchronous build.
    // Model A (#716): the two devices come from the binding registry, not block
    // `entries`.
    let chain = Chain {
        id: ChainId("multi".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
    };
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![
            input_endpoint("inA", "devA"),
            input_endpoint("inB", "devB"),
        ],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("out".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain.clone()],
        midi: None,
    };
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: HashMap::new(),
    });
    controller.set_io_bindings(registry);

    let scheduled = controller
        .schedule_chain_activation(&project, &chain)
        .expect("multi-input query must not error");
    assert!(
        !scheduled,
        "a multi-input chain must fall back to the synchronous activation build"
    );
}
