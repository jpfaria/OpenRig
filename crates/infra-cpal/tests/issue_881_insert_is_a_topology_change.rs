//! #881 — adding an `Insert` to a RUNNING chain must be treated as a STREAM
//! topology change, not a DSP-only edit.
//!
//! An insert needs two streams the chain did not have: its SEND is an output,
//! its RETURN is an input. `sync_live_chain_runtime` asks `chain_io_changed`
//! whether the streams must be rebuilt; that check compared only the chain's
//! OWN bindings, so it answered "unchanged", the edit took the off-thread
//! DSP-only rebuild, and the streams stayed as they were. The segment after
//! the insert then waited on a return stream nobody had opened — the rig went
//! silent, with the graph log showing a perfectly built two-segment runtime.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal \
//!     --test issue_881_insert_is_a_topology_change -- --nocapture
//! ```
#![cfg(target_os = "macos")]

mod hw_harness;

use std::time::Duration;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use infra_cpal::ProjectRuntimeController;
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

const RATE: u32 = 48_000;
const LOOPBACK: &str = "BlackHole";

fn settings(device_id: &str) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(device_id.into()),
        sample_rate: RATE,
        buffer_size_frames: BUFFER,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

#[test]
fn adding_an_insert_to_a_live_chain_is_an_io_change() {
    if !hw_tests_enabled("adding_an_insert_to_a_live_chain_is_an_io_change") {
        return;
    }
    let _guard = device_guard();
    init_registry();

    let device = infra_cpal::list_input_device_descriptors()
        .expect("list inputs")
        .into_iter()
        .find(|d| d.name.contains(LOOPBACK))
        .expect("#881 needs the BlackHole loopback");
    let ep = |name: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.id.clone()),
        mode: ChannelMode::Mono,
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![ep("in", vec![0])],
            outputs: vec![ep("out", vec![0])],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", vec![1])],
            outputs: vec![ep("snd", vec![1])],
        },
    ];

    let chain_id = ChainId("issue-881-topology".into());
    let plain = Chain {
        id: chain_id.clone(),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };
    let project = Project {
        name: Some("issue-881".into()),
        device_settings: vec![settings(&device.id)],
        chains: vec![plain.clone()],
        midi: None,
    };

    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start real streams");
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        if controller.stream_count(&chain_id) > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        controller.stream_count(&chain_id) > 0,
        "the chain must be streaming before the edit — otherwise the check below is vacuous"
    );

    // The user drops a bound insert into the running chain.
    let mut with_insert = plain.clone();
    with_insert.blocks.push(AudioBlock {
        id: BlockId("issue-881:insert".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".into(),
            io: "fx".into(),
        }),
    });

    let changed = controller
        .chain_io_changed(&project, &with_insert)
        .expect("the check must not fail");

    assert!(
        changed,
        "#881: an insert adds a SEND output and a RETURN input — the live-edit \
         path must rebuild the streams. Answering 'unchanged' keeps the old \
         streams and the segment after the insert is never fed: no sound."
    );
}
