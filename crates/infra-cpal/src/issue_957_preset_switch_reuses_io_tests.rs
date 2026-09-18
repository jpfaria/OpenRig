//! #957 — a preset switch on a live chain must not ask CoreAudio for the
//! devices again when its I/O did not move.
//!
//! The switch is a structural edit, so it builds new streams (#881). Resolving
//! the devices for them took 1.1–1.8 s per switch on the owner's Quantum HD 8,
//! against ~40 ms to build the blocks: "travou, está demorando um século para
//! trocar o preset". These open REAL streams (BlackHole).
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --lib issue_957_preset_switch -- --nocapture
//! ```
#![cfg(target_os = "macos")]

use std::time::Duration;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;

use crate::ProjectRuntimeController;

const LOOPBACK: &str = "BlackHole";

fn hw_enabled(name: &str) -> bool {
    if std::env::var("OPENRIG_HW_TESTS").is_ok() {
        return true;
    }
    eprintln!("[{name}] skipped — set OPENRIG_HW_TESTS=1 to run it (opens real streams)");
    false
}

fn settings(device_id: &str, buffer: u32) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(device_id.into()),
        sample_rate: 48_000,
        buffer_size_frames: buffer,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

fn loopback_device() -> Option<String> {
    crate::list_input_device_descriptors()
        .ok()?
        .into_iter()
        .find(|d| d.name.contains(LOOPBACK))
        .map(|d| d.id)
}

fn registry(device: &str) -> Vec<IoBinding> {
    let ep = |name: &str| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    };
    vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![ep("in")],
        outputs: vec![ep("out")],
    }]
}

fn chain_with(block_id: &str) -> Chain {
    Chain {
        id: ChainId("issue-957".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 0.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![AudioBlock {
            id: BlockId(block_id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

fn live(device: &str, buffer: u32) -> (ProjectRuntimeController, Project) {
    let chain = chain_with("preset-a:gain");
    let project = Project {
        name: Some("issue-957".into()),
        device_settings: vec![settings(device, buffer)],
        chains: vec![chain.clone()],
        midi: None,
    };
    let mut controller =
        ProjectRuntimeController::start_with_io_bindings(&project, registry(device))
            .expect("start real streams");
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        if controller.stream_count(&chain.id) > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        controller.stream_count(&chain.id) > 0,
        "the chain must be live first"
    );
    (controller, project)
}

#[test]
fn a_preset_switch_with_the_same_io_reuses_the_live_device_config() {
    if !hw_enabled("a_preset_switch_with_the_same_io_reuses_the_live_device_config") {
        return;
    }
    let Some(device) = loopback_device() else {
        eprintln!("skipped — needs the BlackHole loopback");
        return;
    };
    let (controller, mut project) = live(&device, 128);
    let next = chain_with("preset-b:gain");
    project.chains = vec![next.clone()];
    assert!(
        controller
            .reusable_live_config(&project, &next)
            .expect("the check must not fail")
            .is_some(),
        "#957: same I/O, same device settings — the new streams must be built \
         from the live config, not from another round of CoreAudio queries"
    );
}

#[test]
fn a_device_settings_change_resolves_the_devices_again() {
    if !hw_enabled("a_device_settings_change_resolves_the_devices_again") {
        return;
    }
    let Some(device) = loopback_device() else {
        eprintln!("skipped — needs the BlackHole loopback");
        return;
    };
    let (controller, mut project) = live(&device, 128);
    let next = chain_with("preset-b:gain");
    project.chains = vec![next.clone()];
    project.device_settings = vec![settings(&device, 256)];
    assert!(
        controller
            .reusable_live_config(&project, &next)
            .expect("the check must not fail")
            .is_none(),
        "a new buffer size is not the config the live streams run — resolve again"
    );
}
