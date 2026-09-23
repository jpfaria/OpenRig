//! #967 — switching a chain on must not spend seconds finding its devices.
//!
//! Measured on the owner's rig: switching a chain on took 2.0–2.2 s before
//! audio flowed, every time. Building the DSP took 2 ms and opening the streams
//! 235 ms; the other 1.8–1.9 s went to resolving the chain's devices, because
//! every endpoint (head input, tail output, the insert's send and return) and
//! the channel validation before them walked the whole CoreAudio device list
//! from scratch to find one device by id.
//!
//! This switches a chain on and off on real streams (the BlackHole loopback,
//! so the owner's interface is never touched) and asserts the repeat switch-on
//! is audible within a budget that a full device walk per endpoint cannot meet.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_967_chain_enable_is_fast -- --nocapture
//! ```
#![cfg(target_os = "macos")]

mod hw_harness;

use std::time::{Duration, Instant};

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
/// Opening the loopback's streams costs tens of ms; a device walk per endpoint
/// costs hundreds each on a machine with a few interfaces.
const SWITCH_ON_BUDGET: Duration = Duration::from_millis(600);

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

fn flowing(controller: &ProjectRuntimeController, chain: &ChainId) -> bool {
    controller.stream_count(chain) > 0
        && controller
            .chain_output_route_stats(chain)
            .iter()
            .flat_map(|(_, routes)| routes.iter())
            .any(|r| r.callbacks > 0)
}

/// Switch the chain on the way the app does (cold activation) and return how
/// long until its streams carry audio.
fn switch_on(controller: &mut ProjectRuntimeController, project: &mut Project) -> Duration {
    project.chains[0].enabled = true;
    let chain = project.chains[0].clone();
    let started = Instant::now();
    controller
        .schedule_chain_activation(project, &chain)
        .expect("the activation must schedule");
    while !flowing(controller, &chain.id) {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the chain never started"
        );
        controller.poll_pending_rebuilds();
        std::thread::sleep(Duration::from_millis(2));
    }
    started.elapsed()
}

fn switch_off(controller: &mut ProjectRuntimeController, project: &mut Project) {
    project.chains[0].enabled = false;
    let chain = project.chains[0].clone();
    controller
        .upsert_chain(project, &chain)
        .expect("the switch-off must apply");
}

#[test]
fn switching_a_chain_on_again_does_not_walk_every_device() {
    if !hw_tests_enabled("switching_a_chain_on_again_does_not_walk_every_device") {
        return;
    }
    let _guard = device_guard();
    init_registry();

    let device = infra_cpal::list_input_device_descriptors()
        .expect("list inputs")
        .into_iter()
        .find(|d| d.name.contains(LOOPBACK))
        .expect("#967 needs the BlackHole loopback");
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
    let chain_id = ChainId("issue-967-enable".into());
    let mut project = Project {
        name: Some("issue-967".into()),
        device_settings: vec![settings(&device.id)],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false,
            // The loopback feeds the tail back into the head; volume 0 keeps it
            // silent without changing a single stream.
            volume: 0.0,
            io_binding_ids: vec!["main".into()],
            blocks: vec![AudioBlock {
                id: BlockId("issue-967:insert".into()),
                enabled: true,
                kind: AudioBlockKind::Insert(InsertBlock {
                    model: "external_loop".into(),
                    io: "fx".into(),
                }),
            }],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };
    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start the controller");

    let first = switch_on(&mut controller, &mut project);
    switch_off(&mut controller, &mut project);
    std::thread::sleep(Duration::from_millis(300));
    let mut repeats = Vec::new();
    for _ in 0..3 {
        repeats.push(switch_on(&mut controller, &mut project));
        switch_off(&mut controller, &mut project);
        std::thread::sleep(Duration::from_millis(300));
    }
    eprintln!("[HW] #967 chain on: first {first:?}, again {repeats:?}");

    let worst = repeats.iter().max().copied().unwrap_or_default();
    assert!(
        worst < SWITCH_ON_BUDGET,
        "#967: switching the chain on again took {worst:?} — the devices it \
         already resolved were searched for again across every interface"
    );
}
