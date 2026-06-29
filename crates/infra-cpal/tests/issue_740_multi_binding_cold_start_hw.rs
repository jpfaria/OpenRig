//! Issue #740 — THE owner's symptom at full fidelity: a single rig chain bound
//! to FOUR isolated streams across TWO physical interfaces, brought up cold.
//! Before #740 the four streams came up SERIALLY on the calling thread (the
//! multi-device chain was deferred to the synchronous build), so the first
//! streams ran their callback and counted underruns while the remaining NAM/IR
//! builds still blocked — the owner's boot-time flood
//! (`346752 new underrun(s)` on `rig:input-1`).
//!
//! After #740 the multi-device chain takes the off-thread activation: every
//! per-binding runtime is built on the control worker, then ALL four streams are
//! created and played together in one poll tick. This asserts (1) `start()` does
//! not hold the caller for the four heavy builds and (2) the boot window records
//! ~zero underruns once the four streams are live.
//!
//! Needs TWO physical interfaces, each with >=2 input channels, so it cannot run
//! headless — real-hardware battery (`OPENRIG_HW_TESTS=1`, macOS, #670). The
//! deterministic, hardware-free guard for the same fix lives in
//! `issue_740_multi_binding_cold_start_async`.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_740_multi_binding_cold_start_hw
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

/// Flipping a four-binding rig on may cost a screen frame or two of bookkeeping,
/// never the four serial NAM/IR builds (#693 contract, extended to multi-device).
const CALLER_BUDGET: Duration = Duration::from_millis(150);
/// All four streams must be live within this window (heavy builds run off-thread).
const READY_BUDGET: Duration = Duration::from_secs(15);

/// One isolated input: a single mono channel of `input`, paired with the shared
/// stereo `output`. Four of these (two channels on each of two interfaces) make
/// the owner's four-stream rig.
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
fn four_binding_cold_start_is_async_and_underrun_free() {
    if !hw_tests_enabled("four_binding_cold_start_is_async_and_underrun_free") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(in_a), Some(in_b)) = (inputs.first(), inputs.get(1)) else {
        panic!(
            "issue #740 needs TWO input interfaces (the owner's two-interface rig); \
             found {} input device(s)",
            inputs.len()
        );
    };
    let Some(output) = outputs.first() else {
        panic!("issue #740 needs an output device; found none");
    };
    eprintln!(
        "[#740 REAL] in_a='{}' in_b='{}' out='{}' buffer={BUFFER} — four bindings (2 ch x 2 devices)",
        in_a.name, in_b.name, output.name
    );

    // The owner's shape: ONE chain, FOUR bindings — two mono channels on each of
    // the two interfaces (invariant #4: each is a distinct device+channel tap).
    let registry = vec![
        binding("io-a0", in_a, 0, output),
        binding("io-a1", in_a, 1, output),
        binding("io-b0", in_b, 0, output),
        binding("io-b1", in_b, 1, output),
    ];
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
    let blocks: Vec<AudioBlock> = infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks;
    let chain_id = ChainId("rig".into());
    let project = Project {
        name: Some("issue-740-four-binding".into()),
        device_settings: vec![device_settings(in_a), device_settings(in_b), device_settings(output)],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 0.0,
            io_binding_ids: vec![
                "io-a0".into(),
                "io-a1".into(),
                "io-b0".into(),
                "io-b1".into(),
            ],
            blocks,
        }],
        midi: None,
    };

    // Acceptance #1: the cold bring-up does NOT hold the caller — the four heavy
    // builds run off-thread (#740: multi-device chains take the async path too).
    let t0 = Instant::now();
    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start four-binding rig");
    let caller_elapsed = t0.elapsed();
    eprintln!("[#740 REAL] start() returned in {caller_elapsed:?}");

    // Poll like the GUI tick until all four streams are live.
    let ready_deadline = Instant::now() + READY_BUDGET;
    loop {
        controller.poll_pending_rebuilds();
        if controller.stream_count(&chain_id) == 4 && controller.is_running() {
            break;
        }
        assert!(
            Instant::now() < ready_deadline,
            "four-binding rig never brought up all 4 streams within {READY_BUDGET:?} \
             (live streams: {})",
            controller.stream_count(&chain_id)
        );
        std::thread::sleep(Duration::from_millis(16));
    }
    eprintln!("[#740 REAL] all 4 streams live in {:?}", t0.elapsed());

    assert!(
        caller_elapsed < CALLER_BUDGET,
        "BUG #740: cold-starting a four-binding rig held the caller for \
         {caller_elapsed:?} (budget {CALLER_BUDGET:?}) — the four streams must be \
         built off the calling thread and installed together, not serially inline."
    );

    // Acceptance #2: the boot window records ~zero underruns. Pre-#740 this is
    // where the owner saw 346k+ underruns flood `rig:input-1`.
    let x0 = controller.chain_xrun_count(&chain_id);
    let u0 = controller.chain_underrun_count(&chain_id);
    let settle_until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < settle_until {
        controller.poll_pending_rebuilds();
        std::thread::sleep(Duration::from_millis(16));
    }
    let xruns = controller.chain_xrun_count(&chain_id) - x0;
    let underruns = controller.chain_underrun_count(&chain_id) - u0;

    eprintln!("[#740 REAL] 20s after a four-binding cold start: xruns={xruns} underruns={underruns}");
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "BUG #740: a four-binding (two-interface) rig recorded {xruns} xruns / \
         {underruns} underruns in the 20 s after cold start — the owner's boot \
         underrun flood from bringing the four streams up serially."
    );
}
