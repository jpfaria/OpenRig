//! #967 — switching an insert on or off must not open or close a stream.
//!
//! Measured on the owner's rig before the fix: disabling the SYN-2 insert took
//! 2.1 s before audio flowed again, enabling it 3.0 s, and the callback counters
//! of the chain's other routes reset too — every stream the chain owned was
//! closed and reopened for a one-bit flip. The insert's enable flag was part of
//! the chain's stream topology, so the edit reached `schedule_chain_activation`
//! as a re-bind and got brand-new streams.
//!
//! A bound insert now owns its send and return streams whether it is on or off;
//! the switch is a DSP rebuild on the streams the chain already has. This
//! brings such a chain up on real streams (the BlackHole loopback, so the
//! owner's interface is never touched), switches the insert both ways exactly
//! as `sync_live_chain_runtime` delivers the edit, and asserts not one stream
//! was built.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release --lib \
//!     issue_967_insert_toggle_streams -- --nocapture --test-threads=1
//! ```
#![cfg(target_os = "macos")]

use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

use crate::ProjectRuntimeController;

const RATE: u32 = 48_000;
const BUFFER: u32 = 128;
const LOOPBACK: &str = "BlackHole";
const INSERT: &str = "issue-967:insert";

fn hw_enabled(name: &str) -> bool {
    if std::env::var("OPENRIG_HW_TESTS").is_ok() {
        return true;
    }
    eprintln!("[{name}] skipped — set OPENRIG_HW_TESTS=1 to run it (opens real streams)");
    false
}

fn loopback_device() -> Option<String> {
    crate::list_input_device_descriptors()
        .ok()?
        .into_iter()
        .find(|d| d.name.contains(LOOPBACK))
        .map(|d| d.id)
}

fn project(device: &str, insert_enabled: bool) -> (Project, Vec<IoBinding>) {
    let ep = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![ep("in", 0)],
            outputs: vec![ep("out", 0)],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", 1)],
            outputs: vec![ep("snd", 1)],
        },
    ];
    let project = Project {
        name: Some("issue-967".into()),
        device_settings: vec![DeviceSettings {
            device_id: DeviceId(device.into()),
            sample_rate: RATE,
            buffer_size_frames: BUFFER,
            bit_depth: 32,
            #[cfg(target_os = "linux")]
            realtime: true,
            #[cfg(target_os = "linux")]
            rt_priority: 70,
            #[cfg(target_os = "linux")]
            nperiods: 3,
        }],
        chains: vec![Chain {
            id: ChainId("issue-967-toggle".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            // The loopback feeds the tail back into the head; volume 0 keeps it
            // silent without changing a single stream.
            volume: 0.0,
            io_binding_ids: vec!["main".into()],
            blocks: vec![AudioBlock {
                id: BlockId(INSERT.into()),
                enabled: insert_enabled,
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
    (project, registry)
}

/// (input streams, output streams) the controller holds for the chain.
fn open_streams(controller: &ProjectRuntimeController, chain: &ChainId) -> (usize, usize) {
    controller
        .active_chains
        .get(chain)
        .map(|a| (a._input_streams.len(), a._output_streams.len()))
        .unwrap_or((0, 0))
}

fn tail_is_flowing(controller: &ProjectRuntimeController, chain: &ChainId) -> bool {
    controller
        .chain_output_route_stats(chain)
        .iter()
        .flat_map(|(_, routes)| routes.iter())
        .any(|r| r.route == 0 && r.callbacks > 0)
}

fn send_is_drained(controller: &ProjectRuntimeController, chain: &ChainId) -> bool {
    controller
        .chain_output_route_stats(chain)
        .iter()
        .flat_map(|(_, routes)| routes.iter())
        .any(|r| r.route == 1 && r.callbacks > 0)
}

/// The app's path for an insert switch (`sync_live_chain_runtime`): the edit
/// needs no new streams, so it rebuilds the DSP off the audio thread; returns
/// how long until the rebuilt runtime is live.
fn switch(
    controller: &mut ProjectRuntimeController,
    project: &mut Project,
    enabled: bool,
) -> Duration {
    project.chains[0].blocks[0].enabled = enabled;
    let chain = project.chains[0].clone();
    let started = Instant::now();
    assert!(
        !controller
            .chain_io_changed(project, &chain)
            .expect("io check"),
        "#967: switching the insert read as a re-bind"
    );
    assert!(
        !controller
            .schedule_chain_activation(project, &chain)
            .expect("schedule"),
        "#967: switching the insert was treated as a topology change — brand-new \
         streams, the 2–3 s of silence the owner measured"
    );
    assert!(
        controller
            .request_offthread_rebuild_if_live(project, &chain)
            .expect("live rebuild"),
        "the switch rebuilds the DSP on the live streams"
    );
    while controller.poll_pending_rebuilds() == 0 {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the rebuild never landed"
        );
        std::thread::sleep(Duration::from_micros(200));
    }
    started.elapsed()
}

fn run(start_enabled: bool) {
    let Some(device) = loopback_device() else {
        eprintln!("skipped — needs the BlackHole loopback");
        return;
    };
    let (mut project, registry) = project(&device, start_enabled);
    let chain_id = project.chains[0].id.clone();
    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start real streams");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !(open_streams(&controller, &chain_id) != (0, 0)
        && tail_is_flowing(&controller, &chain_id))
    {
        assert!(Instant::now() < deadline, "the chain never started");
        controller.poll_pending_rebuilds();
        std::thread::sleep(Duration::from_millis(20));
    }
    let streams = open_streams(&controller, &chain_id);
    let generation = controller.stream_generation;
    assert_eq!(
        streams,
        (1, 2),
        "the loop's send is open with the tail whether the insert starts on or off"
    );

    for enabled in [!start_enabled, start_enabled, !start_enabled, start_enabled] {
        let took = switch(&mut controller, &mut project, enabled);
        std::thread::sleep(Duration::from_millis(200));
        controller.poll_pending_rebuilds();
        eprintln!("[HW] #967 insert -> {enabled}: live in {took:?}");

        assert!(
            took < Duration::from_millis(100),
            "#967: the switch must land within a few ms, took {took:?}"
        );
        assert_eq!(
            (
                open_streams(&controller, &chain_id),
                controller.stream_generation
            ),
            (streams, generation),
            "#967: switching the insert must not build a single stream"
        );
        assert!(
            tail_is_flowing(&controller, &chain_id),
            "the tail keeps playing through the switch"
        );
        if enabled {
            assert!(
                send_is_drained(&controller, &chain_id),
                "#967: with the loop on, its send stream must be pulling the send \
                 route — a send stream opened while the loop was off and never \
                 bound to the runtime would leave the gear (and the return) silent"
            );
        }
    }
}

#[test]
fn switching_an_insert_keeps_every_stream_of_the_chain() {
    if !hw_enabled("switching_an_insert_keeps_every_stream_of_the_chain") {
        return;
    }
    run(true);
}

/// The chain comes up with its loop already OFF — a project saved that way, or
/// a chain switched on while its insert is off. The loop's streams are opened
/// all the same, so the first switch-on finds them there.
#[test]
fn a_chain_started_with_its_insert_off_already_owns_the_loops_streams() {
    if !hw_enabled("a_chain_started_with_its_insert_off_already_owns_the_loops_streams") {
        return;
    }
    run(false);
}
