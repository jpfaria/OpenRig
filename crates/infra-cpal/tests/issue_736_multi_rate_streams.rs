//! Issue #736 — THE acceptance evidence: two I/O bindings at DIFFERENT device
//! sample rates (the owner's Scarlett @44.1 kHz + TEYUN @48 kHz) running in the
//! SAME chain, each isolated stream clocked at its own device's rate, with zero
//! xruns / cross-talk.
//!
//! Before #736 this configuration was REJECTED before any stream opened —
//! `unify_io_sample_rates` `bail!`ed with "mismatched sample rates across
//! inputs (44100 vs 48000)" because the chain assumed one engine clock. The fix
//! resolves the rate per binding-group and clocks each per-input runtime at its
//! own device's rate (invariant #4: two isolated streams share no clock).
//!
//! This needs TWO physical interfaces at two different rates, so it cannot run
//! headless — it belongs to the real-hardware battery (`OPENRIG_HW_TESTS=1`,
//! #670). Without the variable it returns immediately with a loud notice (NOT
//! silently green). macOS only, mirroring the rest of the battery.
//!
//! Owner action (cannot be done in CI/headless):
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_736_multi_rate_streams
//! ```
//! with a Scarlett @44.1 kHz and a TEYUN @48 kHz both connected.
#![cfg(target_os = "macos")]

mod hw_harness;

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
use std::path::PathBuf;

const RATE_A: u32 = 44_100;
const RATE_B: u32 = 48_000;

/// One binding = one isolated stream: an input device paired with an output
/// device, both clocked at `rate`.
fn binding(id: &str, input: &AudioDeviceDescriptor, output: &AudioDeviceDescriptor) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: vec![IoEndpoint {
            name: format!("{id}-in"),
            device_id: DeviceId(input.id.clone()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: format!("{id}-out"),
            device_id: DeviceId(output.id.clone()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

fn device_settings(dev: &AudioDeviceDescriptor, rate: u32) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(dev.id.clone()),
        sample_rate: rate,
        buffer_size_frames: BUFFER,
        bit_depth: 32,
    }
}

#[test]
fn two_bindings_at_44k_and_48k_activate_both_streams_no_xruns() {
    if !hw_tests_enabled("two_bindings_at_44k_and_48k_activate_both_streams_no_xruns") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    // Two bindings need two distinct input devices AND two distinct output
    // devices — the whole point of #736 is two physical interfaces at two rates.
    let (Some(in_a), Some(in_b)) = (inputs.first(), inputs.get(1)) else {
        panic!(
            "issue #736 needs TWO input interfaces at different rates \
             (e.g. Scarlett @44.1 kHz + TEYUN @48 kHz); found {} input device(s)",
            inputs.len()
        );
    };
    let (Some(out_a), Some(out_b)) = (outputs.first(), outputs.get(1)) else {
        panic!(
            "issue #736 needs TWO output interfaces at different rates; \
             found {} output device(s)",
            outputs.len()
        );
    };
    eprintln!(
        "[#736 REAL] binding-A in='{}' out='{}' @{RATE_A}  |  binding-B in='{}' out='{}' @{RATE_B}  buffer={BUFFER}",
        in_a.name, out_a.name, in_b.name, out_b.name
    );

    // The same chain, two bindings, two rates. Model A (#716): the device
    // endpoints live in the binding registry, not in block entries.
    let registry = vec![
        binding("io-a", in_a, out_a),
        binding("io-b", in_b, out_b),
    ];
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
    let blocks: Vec<AudioBlock> = infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks;
    let chain_id = ChainId("issue-736-multi-rate".into());
    let project = Project {
        name: Some("issue-736-multi-rate".into()),
        device_settings: vec![
            device_settings(in_a, RATE_A),
            device_settings(out_a, RATE_A),
            device_settings(in_b, RATE_B),
            device_settings(out_b, RATE_B),
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            // Volume 0: identical DSP path (volume applies at output mixdown);
            // just stops blasting the monitors during the battery run.
            volume: 0.0,
            io_binding_ids: vec!["io-a".into(), "io-b".into()],
            blocks,
            di_output: None,
        }],
        midi: None,
    };

    // Acceptance #1: activation must SUCCEED — no "mismatched sample rates"
    // error for the cross-binding rate difference (the pre-#736 failure).
    let mut controller =
        ProjectRuntimeController::start(&project).expect("start real streams (no rate-mismatch)");
    controller.set_io_bindings(registry);
    controller
        .sync_project(&project)
        .expect("resync with bindings (no rate-mismatch across bindings)");

    // Acceptance #2: BOTH isolated streams are live (one per binding).
    std::thread::sleep(std::time::Duration::from_secs(2));
    let streams = controller.stream_count(&chain_id);
    eprintln!("[#736 REAL] live streams for the two-binding chain: {streams}");
    assert_eq!(
        streams, 2,
        "BUG #736: a two-binding chain at {RATE_A}/{RATE_B} Hz must run TWO \
         isolated streams (one per device at its own rate); got {streams}."
    );

    // Acceptance #3: each device clocked at its own rate runs xrun/underrun-free.
    let x0 = controller.chain_xrun_count(&chain_id);
    let u0 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(30));
    let xruns = controller.chain_xrun_count(&chain_id) - x0;
    let underruns = controller.chain_underrun_count(&chain_id) - u0;

    eprintln!(
        "[#736 REAL] 30s of the two-rate chain ({RATE_A} + {RATE_B} Hz): xruns={xruns} underruns={underruns}"
    );
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "BUG #736: the two isolated streams at {RATE_A} Hz and {RATE_B} Hz \
         recorded {xruns} xruns / {underruns} underruns in 30 s — per-stream \
         sample-rate isolation regressed (cross-talk or a shared clock)."
    );
}
