//! Issue #762 — a LIVE re-sync that rebuilds a NAM chain must not hold the
//! calling (GUI) thread.
//!
//! #693 moved the COLD start off-thread, but a re-sync of an already-running
//! rig (a block/param change, a chain toggle, a project reload) still takes the
//! synchronous `upsert_chain_with_resolved` fallback: it constructs every NAM
//! instance inline on the caller. On a multi-NAM rig that is ~90 ms per model —
//! measured as a ~750 ms `[ui-stall]` freeze on the owner's 4-guitar / 8-NAM
//! rig. Each guitar needs its OWN stateful instance, so the builds are
//! unavoidable; they must run on the control worker, like the cold path.
//!
//! Real-hardware battery (opens the physical default devices), idle machine:
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_762_live_rebuild_offthread
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
/// A live re-sync may cost a screen frame of bookkeeping — never the full
/// NAM/stream bring-up.
const CALLER_BUDGET: Duration = Duration::from_millis(150);
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
    enabled: bool,
    input: &infra_cpal::AudioDeviceDescriptor,
    output: &infra_cpal::AudioDeviceDescriptor,
) -> (Project, ChainId) {
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
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
    let chain_id = ChainId("issue-762-live".into());
    let project = Project {
        name: Some("issue-762-live-rebuild".into()),
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
            enabled,
            volume: 0.0, // identical DSP path, silent monitors
            blocks,
        }],
        midi: None,
    };
    (project, chain_id)
}

fn hw_enabled() -> bool {
    if std::env::var_os("OPENRIG_HW_TESTS").is_some() {
        return true;
    }
    eprintln!(
        "[#762 HW] SKIPPED — real-hardware timing test. Run with OPENRIG_HW_TESTS=1 on an idle machine."
    );
    false
}

#[test]
fn live_rebuild_does_not_hold_the_caller() {
    if !hw_enabled() {
        return;
    }
    init_registry();
    let inputs = list_input_device_descriptors().expect("inputs");
    let outputs = list_output_device_descriptors().expect("outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        eprintln!("[#762 HW] no audio devices — skipping");
        return;
    };

    // Bring the rig up first (cold start is already async per #693).
    let (project, chain_id) = rig_project(true, input, output);
    let mut controller = ProjectRuntimeController::start(&project).expect("start");
    let ready = Instant::now() + READY_BUDGET;
    while !(controller.chain_runtime(&chain_id).is_some() && controller.is_running()) {
        controller.poll_pending_rebuilds();
        assert!(Instant::now() < ready, "chain never became ready");
        std::thread::sleep(Duration::from_millis(16));
    }

    // Now a LIVE re-sync that forces a rebuild (toggle the chain off then on —
    // the enable re-instantiates every NAM). The caller must return fast; the
    // NAM construction belongs on the control worker.
    let (off, _) = rig_project(false, input, output);
    controller.sync_project(&off).expect("sync off");
    let (on, _) = rig_project(true, input, output);

    let t0 = Instant::now();
    controller.sync_project(&on).expect("sync on");
    let caller = t0.elapsed();
    eprintln!("[#762 HW] live re-enable sync returned in {caller:?}");

    assert!(
        caller < CALLER_BUDGET,
        "live re-sync held the caller for {caller:?} (budget {CALLER_BUDGET:?}) — the \
         NAM rebuild must run on the control worker, not the calling (GUI) thread (#762)"
    );
}
