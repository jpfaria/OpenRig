//! #967 — switching an insert on or off must not touch the chain's streams.
//!
//! Measured on the owner's rig before the fix: disabling the SYN-2 insert took
//! 2.1 s before audio flowed again, enabling it 3.0 s, and the callback counters
//! of the chain's OTHER routes reset too — every stream the chain owned was
//! closed and reopened for a one-bit flip. The insert's `enabled` flag was part
//! of the chain's structure, so the edit reached `schedule_chain_activation` as
//! a topology change and got brand-new streams.
//!
//! This brings a chain with a bound insert up on real streams (the BlackHole
//! loopback, so the owner's interface is never touched), flips the insert both
//! ways exactly as the app delivers the edit, and asserts the streams the chain
//! started with are the ones still running.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_967_insert_toggle_keeps_streams -- --nocapture
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
const INSERT: &str = "issue-967:insert";

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

/// `(route, channels, callbacks)` of every output route the chain's streams
/// serve — the counters only ever grow while the SAME streams run.
fn routes(controller: &ProjectRuntimeController, chain: &ChainId) -> Vec<(usize, Vec<usize>, u64)> {
    controller
        .chain_output_route_stats(chain)
        .into_iter()
        .flat_map(|(_, stats)| stats.into_iter().map(|s| (s.route, s.channels, s.callbacks)))
        .collect()
}

fn shape(rows: &[(usize, Vec<usize>, u64)]) -> Vec<(usize, Vec<usize>)> {
    rows.iter().map(|(r, c, _)| (*r, c.clone())).collect()
}

/// Deliver a toggle the way the app does now: ask whether the edit needs new
/// streams (the question whose "yes" cost 2–3 s), then flip the insert live.
fn toggle(controller: &mut ProjectRuntimeController, project: &mut Project, enabled: bool) -> Duration {
    project.chains[0].blocks[1].enabled = enabled;
    let chain = project.chains[0].clone();
    let started = Instant::now();
    let new_streams = controller
        .schedule_chain_activation(project, &chain)
        .expect("the activation check must not fail");
    assert!(
        !new_streams,
        "#967: switching the insert {} was treated as a topology change — the \
         chain gets brand-new streams, the 2–3 s of silence the owner measured",
        if enabled { "on" } else { "off" }
    );
    controller
        .toggle_block_enabled_live(&chain, &BlockId(INSERT.into()), enabled)
        .expect("the live toggle must apply");
    started.elapsed()
}

#[test]
fn switching_an_insert_keeps_every_stream_of_the_chain() {
    if !hw_tests_enabled("switching_an_insert_keeps_every_stream_of_the_chain") {
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

    let chain_id = ChainId("issue-967-insert-toggle".into());
    let gain = AudioBlock {
        id: BlockId("issue-967:gain".into()),
        enabled: true,
        kind: AudioBlockKind::Core(project::block::CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: project::param::ParameterSet::default(),
        }),
    };
    let insert = AudioBlock {
        id: BlockId(INSERT.into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".into(),
            io: "fx".into(),
        }),
    };
    let mut project = Project {
        name: Some("issue-967".into()),
        device_settings: vec![settings(&device.id)],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            // The loopback feeds the tail back into the head input; volume 0
            // keeps that silent without changing a single stream.
            volume: 0.0,
            io_binding_ids: vec!["main".into()],
            blocks: vec![gain, insert],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };

    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start real streams");
    let settle = |controller: &mut ProjectRuntimeController, for_ms: u64| {
        let until = Instant::now() + Duration::from_millis(for_ms);
        while Instant::now() < until {
            controller.poll_pending_rebuilds();
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        if controller.stream_count(&chain_id) > 0 && routes(&controller, &chain_id).len() >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    settle(&mut controller, 300);

    let streams = controller.stream_count(&chain_id);
    let mut before = routes(&controller, &chain_id);
    assert!(
        streams > 0 && before.len() >= 2,
        "the chain must be streaming with its send and tail routes before the edit \
         — otherwise the checks below are vacuous (streams={streams}, routes={before:?})"
    );

    for enabled in [false, true, false, true] {
        let took = toggle(&mut controller, &mut project, enabled);
        settle(&mut controller, 400);
        let after = routes(&controller, &chain_id);
        eprintln!("[HW] #967 insert {enabled}: applied in {took:?}, routes {after:?}");

        assert!(
            took < Duration::from_millis(5),
            "#967: the toggle must be applied on the spot, took {took:?}"
        );
        assert_eq!(
            controller.stream_count(&chain_id),
            streams,
            "#967: switching the insert must not open or close a stream"
        );
        assert_eq!(
            shape(&after),
            shape(&before),
            "#967: the send route must stay while the insert is off — it is bypassed, not unplugged"
        );
        for ((route, _, was), (_, _, now)) in before.iter().zip(after.iter()) {
            assert!(
                now > was,
                "#967: route {route} restarted (callbacks {was} → {now}) — the chain's \
                 streams were rebuilt for a one-bit flip"
            );
        }
        before = after;
    }
}
