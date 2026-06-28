//! Issue #672 — cold activation. `schedule_chain_activation` builds a chain's
//! runtime off the frontend thread (the NAM/IR load — the bulk of the activation
//! freeze) and returns true; the frontend poll then creates the cpal streams
//! (which are `!Send`, so they must stay on the frontend) and installs the chain.
//!
//! Issue #740: a multi-input / multi-device chain is scheduled off-thread TOO.
//! It used to return false and fall back to the synchronous serial build, which
//! brought every stream up one at a time on the calling thread and starved the
//! first streams while the rest loaded (the owner's four-binding boot underrun
//! flood). The off-thread path already builds one runtime per input group and
//! one stream per device, so the multi-device case takes it as well. That guard
//! runs before any device probe, so it is testable without hardware.

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
fn schedules_a_multi_input_chain_off_thread() {
    // Two distinct input devices => two per-device runtimes. #740: the off-thread
    // path builds both runtimes and installs both streams, so a multi-device
    // chain is scheduled async (true) instead of deferring to the synchronous
    // serial build. Model A (#716): the two devices come from the binding
    // registry, not block `entries`.
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
        scheduled,
        "#740: a multi-input chain must be scheduled off-thread (async), not \
         fall back to the synchronous serial build"
    );
}
