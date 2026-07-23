//! Issue #693 — cold runtime start must not hold the calling thread.
//!
//! `ProjectRuntimeController::start` is what runs when the user flips
//! the first chain on (and on project open via the GUI wiring): today
//! it brings up every enabled chain INLINE — NAM/IR loads, route
//! assembly, stream opening — measured at 1.6 s (develop) to 2.6 s
//! (#670 branch) for a real 4-chain project, all spent on the GUI
//! thread. Under the #693 contract every action is its own task: the
//! caller gets the controller back immediately and the bring-up
//! completes asynchronously (control worker + poll tick), exactly like
//! the #672 off-thread rebuild path.
//!
//! Real-hardware battery test (opens the physical default devices):
//! run on an idle machine with
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_693_cold_start_must_not_block_caller
//! ```
#![cfg(all(target_os = "macos", not(debug_assertions)))]

use std::path::PathBuf;
use std::sync::Once;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::device::DeviceSettings;
use project::project::Project;

const BUFFER: u32 = 64;

/// Caller-side budget: flipping a chain on may cost a screen frame or
/// two of bookkeeping, never the full DSP/stream bring-up.
const CALLER_BUDGET: Duration = Duration::from_millis(150);
/// The bring-up itself must complete (sound ready) within this window.
const READY_BUDGET: Duration = Duration::from_secs(5);

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        lv2::register_builder();
        block_dyn::register_natives();
        block_filter::register_natives();
        block_reverb::register_natives();
        block_gain::register_natives();
        block_amp::register_natives();
        block_preamp::register_natives();
        block_cab::register_natives();
        block_delay::register_natives();
        block_mod::register_natives();
        block_pitch::register_natives();
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../engine/tests/fixtures/plugins");
        plugin_loader::registry::init(&root);
    });
}

fn rig_project(
    preset_file: &str,
    input: &infra_cpal::AudioDeviceDescriptor,
    output: &infra_cpal::AudioDeviceDescriptor,
) -> (Project, ChainId) {
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join(preset_file);
    let mut blocks = vec![AudioBlock {
        id: BlockId("in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![InputEntry {
                device_id: DeviceId(input.id.clone()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }];
    blocks.extend(
        infra_yaml::load_chain_preset_file(&preset)
            .expect("preset")
            .blocks,
    );
    blocks.push(AudioBlock {
        id: BlockId("out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![OutputEntry {
                device_id: DeviceId(output.id.clone()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    });
    let chain_id = ChainId("issue-693-cold".into());
    let project = Project {
        name: Some("issue-693-cold-start".into()),
        device_settings: vec![
            DeviceSettings {
                device_id: DeviceId(input.id.clone()),
                sample_rate: 48_000,
                buffer_size_frames: BUFFER,
                bit_depth: 32,
                #[cfg(target_os = "linux")]
                realtime: true,
                #[cfg(target_os = "linux")]
                rt_priority: 70,
                #[cfg(target_os = "linux")]
                nperiods: 3,
            },
            DeviceSettings {
                device_id: DeviceId(output.id.clone()),
                sample_rate: 48_000,
                buffer_size_frames: BUFFER,
                bit_depth: 32,
                #[cfg(target_os = "linux")]
                realtime: true,
                #[cfg(target_os = "linux")]
                rt_priority: 70,
                #[cfg(target_os = "linux")]
                nperiods: 3,
            },
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            // Volume 0: identical DSP path, silent monitors.
            volume: 0.0,
            blocks,
        }],
        midi: None,
    };
    (project, chain_id)
}

fn hw_tests_enabled(test_name: &str) -> bool {
    if std::env::var_os("OPENRIG_HW_TESTS").is_some() {
        return true;
    }
    eprintln!(
        "[#693 HW] {test_name}: SKIPPED — real-hardware timing test. \
         Run with OPENRIG_HW_TESTS=1 on an idle machine (docs/testing.md)."
    );
    false
}

#[test]
fn issue_693_cold_start_returns_within_caller_budget() {
    if !hw_tests_enabled("issue_693_cold_start_returns_within_caller_budget") {
        return;
    }
    init_registry();
    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let input = inputs.first();
    let output = outputs.first();
    let (Some(input), Some(output)) = (input, output) else {
        eprintln!("[#693 HW] no audio devices — skipping");
        return;
    };
    let (project, chain_id) = rig_project("beat_it_michael_jackson_rhythm.yaml", input, output);

    let t0 = Instant::now();
    let mut controller = ProjectRuntimeController::start(&project).expect("start");
    let caller_elapsed = t0.elapsed();
    eprintln!("[#693 HW] start() returned in {caller_elapsed:?}");

    // The bring-up must still complete: poll like the GUI tick does.
    let ready_deadline = Instant::now() + READY_BUDGET;
    loop {
        controller.poll_pending_rebuilds();
        if controller.chain_runtime(&chain_id).is_some() && controller.is_running() {
            break;
        }
        assert!(
            Instant::now() < ready_deadline,
            "chain never became ready within {READY_BUDGET:?}"
        );
        std::thread::sleep(Duration::from_millis(16));
    }
    eprintln!("[#693 HW] chain ready in {:?}", t0.elapsed());

    assert!(
        caller_elapsed < CALLER_BUDGET,
        "ProjectRuntimeController::start held the caller for {caller_elapsed:?} \
         (budget {CALLER_BUDGET:?}) — the cold bring-up must run as its own \
         task, not on the calling (GUI) thread (issue #693)"
    );
}
