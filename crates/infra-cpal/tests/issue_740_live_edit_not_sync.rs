//! Issue #740 (cont.) — a LIVE edit on a running chain must not block the GUI
//! thread on a synchronous CoreAudio resolve. Opening a chain is async now, but
//! the owner reports that switching a preset / toggling a block / changing a
//! block parameter still feels synchronous: `upsert_chain` runs
//! `resolve_chain_audio_config` (a device query costing hundreds of ms) on the
//! calling thread every edit, even when the chain's I/O did not change.
//!
//! Brings up the owner's two-interface rig, then times a parameter edit
//! (`upsert_chain` with one NAM param changed, I/O unchanged). The edit must
//! return within a frame budget — a multi-hundred-ms block is the freeze the
//! owner feels. Real-hardware battery (`OPENRIG_HW_TESTS=1`, macOS release).
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, AudioDeviceDescriptor,
    ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

/// A live edit may cost a frame of bookkeeping, never a device resolve.
const EDIT_BUDGET: Duration = Duration::from_millis(120);

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
fn a_live_param_edit_does_not_block_on_a_device_resolve() {
    if !hw_tests_enabled("a_live_param_edit_does_not_block_on_a_device_resolve") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("inputs");
    let outputs = list_output_device_descriptors().expect("outputs");
    let find_in = |n: &str| inputs.iter().find(|d| d.name.contains(n));
    let find_out = |n: &str| outputs.iter().find(|d| d.name.contains(n));
    let (Some(scar_in), Some(tey_in), Some(scar_out), Some(tey_out)) = (
        find_in("Scarlett"),
        find_in("TEYUN"),
        find_out("Scarlett"),
        find_out("TEYUN"),
    ) else {
        eprintln!("[#740 HW] SKIPPED — needs Scarlett + TEYUN connected.");
        return;
    };

    let registry = vec![
        interface("io-scar", scar_in, scar_out),
        interface("io-tey", tey_in, tey_out),
    ];
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
    let blocks: Vec<AudioBlock> = infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks;
    let chain_id = ChainId("rig".into());
    let project = Project {
        name: Some("issue-740-live-edit".into()),
        device_settings: vec![
            settings(scar_in, 44_100),
            settings(scar_out, 44_100),
            settings(tey_in, 48_000),
            settings(tey_out, 48_000),
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

    let mut controller =
        ProjectRuntimeController::start_with_io_bindings(&project, registry).expect("start");
    let ready = Instant::now() + Duration::from_secs(15);
    while !(controller.stream_count(&chain_id) == 4 && controller.is_running()) {
        controller.poll_pending_rebuilds();
        assert!(Instant::now() < ready, "rig never came up");
        std::thread::sleep(Duration::from_millis(16));
    }
    std::thread::sleep(Duration::from_secs(1));

    // A live parameter edit, I/O unchanged: change one NAM output_db.
    let mut edited = project.clone();
    {
        let nam = edited.chains[0]
            .blocks
            .iter_mut()
            .find(|b| matches!(&b.kind, AudioBlockKind::Core(c) if c.model.starts_with("nam_")))
            .expect("a NAM block");
        if let AudioBlockKind::Core(c) = &mut nam.kind {
            c.params.insert("output_db", ParameterValue::Float(-1.0));
        }
    }

    // The live-edit path the GUI now takes for a running chain (#740): rebuild
    // off-thread, no synchronous resolve / NAM reload on the caller.
    let t = Instant::now();
    let scheduled = controller
        .request_offthread_rebuild_if_live(&edited, &edited.chains[0])
        .expect("live param edit");
    let blocked = t.elapsed();
    eprintln!(
        "[#740 REAL] live param edit (I/O unchanged): scheduled={scheduled} blocked={blocked:?}"
    );

    assert!(
        scheduled,
        "an I/O-unchanged live edit on a running chain must rebuild off-thread"
    );
    assert!(
        blocked < EDIT_BUDGET,
        "BUG #740: a live parameter edit blocked the calling (GUI) thread for {blocked:?} \
         (budget {EDIT_BUDGET:?}) — the live-edit path must reuse the running stream config and \
         rebuild the DSP off-thread, not resolve devices + reload NAM synchronously."
    );

    // The off-thread rebuild must actually complete (sound stays live).
    let ready = Instant::now() + Duration::from_secs(10);
    while controller.poll_pending_rebuilds() == 0 && Instant::now() < ready {
        std::thread::sleep(Duration::from_millis(16));
    }
    assert_eq!(
        controller.stream_count(&chain_id),
        4,
        "streams stay live across the off-thread edit"
    );
}
