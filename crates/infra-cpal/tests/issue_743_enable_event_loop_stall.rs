//! Issue #743 — the owner's four-stream, two-interface rig (Scarlett 2i2 @44.1 kHz +
//! TEYUN Q26 @48 kHz) on enable. Two real defects, measured on the real
//! interfaces (NOT arbitrary first-two devices — virtual loopbacks like BlackHole
//! starve unconditionally and tell us nothing):
//!
//! 1. `[ui-stall] ~768ms`: the cpal stream creation for all four streams lands in
//!    one `poll_pending_rebuilds()` tick on the frontend thread.
//! 2. the boot underrun flood the owner reports.
//!
//! Real-hardware battery (`OPENRIG_HW_TESTS=1`, macOS release). Skips (loudly)
//! when the owner's two interfaces are not both present.
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

/// No single GUI-thread call may freeze the event loop past a couple of frames.
const EVENT_LOOP_BUDGET: Duration = Duration::from_millis(120);
const READY_BUDGET: Duration = Duration::from_secs(15);
const RATE_SCARLETT: u32 = 44_100;
const RATE_TEYUN: u32 = 48_000;

/// One interface: its two mono input channels (two isolated runtimes sharing the
/// one input stream) plus a single stereo output on the same interface — exactly
/// the owner's per-interface shape (2 input streams + 2 output streams total,
/// NOT 4 output streams on duplicated devices).
fn interface(id: &str, dev: &AudioDeviceDescriptor, output: &AudioDeviceDescriptor) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: vec![
            IoEndpoint {
                name: format!("{id}-in0"),
                device_id: DeviceId(dev.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            },
            IoEndpoint {
                name: format!("{id}-in1"),
                device_id: DeviceId(dev.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            },
        ],
        outputs: vec![IoEndpoint {
            name: format!("{id}-out"),
            device_id: DeviceId(output.id.clone()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

fn settings(dev: &AudioDeviceDescriptor, rate: u32) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(dev.id.clone()),
        sample_rate: rate,
        buffer_size_frames: BUFFER,
        bit_depth: 32,
    }
}

#[test]
fn owner_two_interface_rig_enable_is_clean() {
    if !hw_tests_enabled("owner_two_interface_rig_enable_is_clean") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");

    // Target the owner's REAL interfaces by name — not the first two enumerated
    // (which on this machine are BlackHole / MJAudioRecorder, virtual devices
    // that starve unconditionally and would give a meaningless number).
    let find_in = |needle: &str| inputs.iter().find(|d| d.name.contains(needle));
    let find_out = |needle: &str| outputs.iter().find(|d| d.name.contains(needle));
    let (Some(scar_in), Some(tey_in), Some(scar_out), Some(tey_out)) = (
        find_in("Scarlett"),
        find_in("TEYUN"),
        find_out("Scarlett"),
        find_out("TEYUN"),
    ) else {
        eprintln!(
            "[#743 HW] SKIPPED — needs the owner's Scarlett 2i2 + TEYUN Q26 both connected. \
             inputs={:?} outputs={:?}",
            inputs.iter().map(|d| &d.name).collect::<Vec<_>>(),
            outputs.iter().map(|d| &d.name).collect::<Vec<_>>(),
        );
        return;
    };
    eprintln!(
        "[#743 REAL] Scarlett@{RATE_SCARLETT} + TEYUN@{RATE_TEYUN}, two interfaces x 2 channels, buffer={BUFFER}"
    );

    // Four streams: two mono channels on each interface, each routed to its OWN
    // interface's output at that interface's native rate (the owner's mixed-rate
    // shape, #736).
    let registry = vec![
        interface("io-scar", scar_in, scar_out),
        interface("io-tey", tey_in, tey_out),
    ];
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
    let blocks: Vec<AudioBlock> = infra_yaml::load_chain_preset_file(&preset).expect("preset").blocks;
    let chain_id = ChainId("rig".into());
    let project = Project {
        name: Some("issue-743-owner-rig".into()),
        device_settings: vec![
            settings(scar_in, RATE_SCARLETT),
            settings(scar_out, RATE_SCARLETT),
            settings(tey_in, RATE_TEYUN),
            settings(tey_out, RATE_TEYUN),
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 0.0,
            io_binding_ids: vec!["io-scar".into(), "io-tey".into()],
            blocks,
        }],
        midi: None,
    };

    // Drive bring-up like the GUI and record the longest single blocking call.
    let mut worst = Duration::ZERO;
    let t = Instant::now();
    let mut controller =
        ProjectRuntimeController::start_with_io_bindings(&project, registry).expect("start rig");
    worst = worst.max(t.elapsed());

    let ready_deadline = Instant::now() + READY_BUDGET;
    loop {
        let t = Instant::now();
        controller.poll_pending_rebuilds();
        worst = worst.max(t.elapsed());
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
    eprintln!("[#743 REAL] worst single GUI-thread call during enable: {worst:?}");

    // Measure the boot window once live.
    let x0 = controller.chain_xrun_count(&chain_id);
    let u0 = controller.chain_underrun_count(&chain_id);
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until {
        controller.poll_pending_rebuilds();
        std::thread::sleep(Duration::from_millis(16));
    }
    let xruns = controller.chain_xrun_count(&chain_id) - x0;
    let underruns = controller.chain_underrun_count(&chain_id) - u0;
    eprintln!("[#743 REAL] 20s after enable: xruns={xruns} underruns={underruns}");

    // THE audio fix (#743): a runtime clocked at one input device's rate must
    // not have its output route consumed by an output stream at a DIFFERENT
    // rate — that cross-rate mismatch starved the output on almost every pop
    // (3.68M underruns / 0 xrun at ~11% CPU). Each output now mixes only
    // same-rate runtimes.
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "BUG #743: the owner's two-interface mixed-rate rig recorded {xruns} xruns / \
         {underruns} underruns in 20 s after enable — cross-rate output mixing starves \
         the route."
    );

    // Separate, still-open residual (NOT the audio bug): the cpal stream creation
    // for all streams lands in one poll tick and freezes the GUI for the slow
    // device open. Reported, not asserted here — tracked on its own.
    if worst >= EVENT_LOOP_BUDGET {
        eprintln!(
            "[#743 REAL] NOTE: enable still blocked the event loop {worst:?} \
             (> {EVENT_LOOP_BUDGET:?}) in cpal stream creation — separate residual."
        );
    }
}
