//! Issue #743 — turning a four-stream, two-interface rig ON must not freeze the
//! GUI event loop. The owner's log shows `[ui-stall] event loop unresponsive for
//! ~768ms` on every enable: the heavy DSP build is already off-thread (#740/#693),
//! but the cpal stream creation + CoreAudio workgroup joins run in a SINGLE
//! `poll_pending_rebuilds()` tick on the frontend thread, and that one call blocks
//! for ~768 ms — long enough that the streams that are already live starve
//! (the boot underruns) and the UI hangs.
//!
//! This pins the contract the owner actually feels: no single call the GUI makes
//! during bring-up may block the event loop past a frame budget. `start()` and
//! EACH `poll_pending_rebuilds()` are timed; the longest must stay under budget.
//!
//! Needs TWO physical interfaces, so it is real-hardware battery only
//! (`OPENRIG_HW_TESTS=1`, macOS release). Run on an idle machine:
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_743_enable_event_loop_stall
//! ```
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, AudioDeviceDescriptor,
    ProjectRuntimeController,
};
use project::block::AudioBlock;
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

/// One GUI tick must never block the event loop past a couple of frames. The
/// owner saw ~768 ms; a healthy bring-up tick is well under this.
const EVENT_LOOP_BUDGET: Duration = Duration::from_millis(120);
/// All streams must be live within this many polls of headroom.
const READY_BUDGET: Duration = Duration::from_secs(15);

fn binding(id: &str, input: &AudioDeviceDescriptor, channel: usize, output: &AudioDeviceDescriptor) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: vec![IoEndpoint {
            name: format!("{id}-in"),
            device_id: DeviceId(input.id.clone()),
            mode: ChannelMode::Mono,
            channels: vec![channel],
        }],
        outputs: vec![IoEndpoint {
            name: format!("{id}-out"),
            device_id: DeviceId(output.id.clone()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

fn device_settings(dev: &AudioDeviceDescriptor) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(dev.id.clone()),
        sample_rate: 48_000,
        buffer_size_frames: BUFFER,
        bit_depth: 32,
    }
}

#[test]
fn enabling_a_four_stream_rig_never_stalls_the_event_loop() {
    if !hw_tests_enabled("enabling_a_four_stream_rig_never_stalls_the_event_loop") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(in_a), Some(in_b)) = (inputs.first(), inputs.get(1)) else {
        panic!("issue #743 needs TWO input interfaces; found {}", inputs.len());
    };
    let Some(output) = outputs.first() else {
        panic!("issue #743 needs an output device; found none");
    };

    let registry = vec![
        binding("io-a0", in_a, 0, output),
        binding("io-a1", in_a, 1, output),
        binding("io-b0", in_b, 0, output),
        binding("io-b1", in_b, 1, output),
    ];
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
    let blocks: Vec<AudioBlock> = infra_yaml::load_chain_preset_file(&preset).expect("preset").blocks;
    let chain_id = ChainId("rig".into());
    let project = Project {
        name: Some("issue-743-event-loop".into()),
        device_settings: vec![device_settings(in_a), device_settings(in_b), device_settings(output)],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 0.0,
            io_binding_ids: vec!["io-a0".into(), "io-a1".into(), "io-b0".into(), "io-b1".into()],
            blocks,
        }],
        midi: None,
    };

    // Drive the bring-up exactly like the GUI: start, then poll every frame.
    // Time the start() call and EVERY poll tick; the event loop is frozen for
    // however long the longest single call takes.
    let mut worst = Duration::ZERO;
    let mut worst_where = "start()";

    let t = Instant::now();
    let mut controller =
        ProjectRuntimeController::start_with_io_bindings(&project, registry).expect("start rig");
    let start_blocked = t.elapsed();
    if start_blocked > worst {
        worst = start_blocked;
        worst_where = "start()";
    }

    let ready_deadline = Instant::now() + READY_BUDGET;
    loop {
        let t = Instant::now();
        controller.poll_pending_rebuilds();
        let tick = t.elapsed();
        if tick > worst {
            worst = tick;
            worst_where = "poll_pending_rebuilds()";
        }
        if controller.stream_count(&chain_id) == 4 && controller.is_running() {
            break;
        }
        assert!(
            Instant::now() < ready_deadline,
            "rig never brought up all 4 streams within {READY_BUDGET:?} (live: {})",
            controller.stream_count(&chain_id)
        );
        std::thread::sleep(Duration::from_millis(16));
    }

    eprintln!(
        "[#743 REAL] worst single GUI-thread call during enable: {worst:?} in {worst_where}"
    );
    assert!(
        worst < EVENT_LOOP_BUDGET,
        "BUG #743: enabling the four-stream rig blocked the GUI event loop for \
         {worst:?} in {worst_where} (budget {EVENT_LOOP_BUDGET:?}) — the owner's \
         ~768 ms [ui-stall]. No single bring-up call may freeze the event loop: \
         the cpal stream creation / workgroup joins must not all land in one tick."
    );
}
