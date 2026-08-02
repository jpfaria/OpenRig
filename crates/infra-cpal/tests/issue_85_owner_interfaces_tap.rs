//! Issue #85 — the owner's OWN interfaces, not the loopback: his Scarlett at
//! 44.1 kHz clocking the chain and his TEYUN at 48 kHz taking the mid `Output`.
//! Every other file in this battery routes through BlackHole, which shares the
//! machine's clock; two real USB interfaces do not, and that is where his
//! "0 xrun(s), 256 new underrun(s)" lives.
//!
//! No loopback to listen on here — the oracle is the chain's own counters:
//! underruns mean the tap's route was fed too slowly, xruns mean the callback
//! missed its deadline. The rig plays a DI loop so no one has to hold a guitar.
//!
//! Needs BOTH interfaces connected and the app CLOSED (they are opened here).
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_85_owner_interfaces_tap -- --nocapture
//! ```
#![cfg(target_os = "macos")]

mod hw_harness;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind, OutputBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

/// The chain's clock.
const CHAIN_RATE: u32 = 44_100;
/// The tap device's clock — deliberately different.
const TAP_RATE: u32 = 48_000;
const CHAIN_IFACE: &str = "Scarlett";
const TAP_IFACE: &str = "TEYUN";

fn settings(device_id: &str, rate: u32) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(device_id.into()),
        sample_rate: rate,
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

fn di_tone() -> Arc<engine::DiPcm> {
    let frames = CHAIN_RATE as usize;
    let step = 2.0 * std::f32::consts::PI * 440.0 / CHAIN_RATE as f32;
    let samples: Vec<f32> = (0..frames).map(|i| 0.5 * (i as f32 * step).sin()).collect();
    Arc::new(engine::DiPcm::new(samples, CHAIN_RATE, 1))
}

/// The WHOLE preset — the load his hard-rock rig carries.
fn heavy_blocks() -> Vec<AudioBlock> {
    let preset = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
    infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks
}

fn wire_block() -> AudioBlock {
    let preset = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
    infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks
        .into_iter()
        .next()
        .expect("preset has at least one block")
}

/// `heavy` swaps the single wire block for the whole preset — the difference
/// between "liguei e estava bom" and "coloquei o preset do hardrock".
fn run(heavy: bool, mid_input: bool) -> (u64, u64) {
    let _ = env_logger::try_init();
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let loop_in = inputs
        .iter()
        .find(|d| d.name.contains(CHAIN_IFACE))
        .expect("#85 needs the Scarlett INPUT connected");
    let loop_out = outputs
        .iter()
        .find(|d| d.name.contains(TAP_IFACE))
        .expect("#85 needs the TEYUN OUTPUT connected");
    // The chain's own I/O: the loopback clocks the input (the DI replaces the
    // samples) and a silent virtual device takes the tail, so the only thing
    // reaching the loopback OUTPUT is the tap.
    let chain_out = outputs
        .iter()
        .find(|d| d.name.contains(CHAIN_IFACE))
        .expect("#85 needs the Scarlett OUTPUT connected");

    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId(loop_in.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId(chain_out.id.clone()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        },
        IoBinding {
            id: "aux2".into(),
            name: "SCARLET - 2".into(),
            // His mid `Input` reads the SECOND channel of the same interface.
            inputs: vec![IoEndpoint {
                name: "in1".into(),
                device_id: DeviceId(loop_in.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            }],
            outputs: Vec::new(),
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: Vec::new(),
            outputs: vec![IoEndpoint {
                name: "aux-out".into(),
                device_id: DeviceId(loop_out.id.clone()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        },
    ];

    let chain_id = ChainId("issue-85-other-rate-real".into());
    let project = Project {
        name: Some("issue-85-other-rate-real".into()),
        device_settings: vec![
            settings(&loop_in.id, CHAIN_RATE),
            settings(&chain_out.id, CHAIN_RATE),
            // The tap's device runs at its own rate — the whole point.
            settings(&loop_out.id, TAP_RATE),
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["main".into()],
            blocks: {
                let mut blocks = if heavy {
                    heavy_blocks()
                } else {
                    vec![wire_block()]
                };
                blocks.push(AudioBlock {
                    id: BlockId("issue85:mid-output".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".into(),
                        io: "aux".into(),
                        endpoint: "aux-out".into(),
                    }),
                });
                // The mid Output ends up one block from the end, where he put
                // his (after the cab).
                let last = blocks.len() - 1;
                blocks.swap(last, last.saturating_sub(1));
                if mid_input {
                    // …and his mid `Input` sits inside the preset, so the chain
                    // owns 2 inputs × 2 outputs = FOUR pipelines.
                    let at = blocks.len() / 3;
                    blocks.insert(
                        at,
                        AudioBlock {
                            id: BlockId("issue85:mid-input".into()),
                            enabled: true,
                            kind: AudioBlockKind::Input(project::block::InputBlock {
                                model: "standard".into(),
                                io: "aux2".into(),
                                endpoint: "in1".into(),
                            }),
                        },
                    );
                }
                blocks
            },
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };

    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start real streams");
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        if controller.stream_count(&chain_id) > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    controller.set_chain_di_loop(&chain_id, Some(di_tone()));
    std::thread::sleep(Duration::from_secs(2));

    let under0 = controller.chain_underrun_count(&chain_id);
    let xrun0 = controller.chain_xrun_count(&chain_id);
    std::thread::sleep(Duration::from_secs(10));
    let underruns = controller.chain_underrun_count(&chain_id) - under0;
    let xruns = controller.chain_xrun_count(&chain_id) - xrun0;
    let load = controller.chain_peak_load(&chain_id);
    controller.set_chain_di_loop(&chain_id, None);

    eprintln!(
        "[#85 OWNER] {} — Scarlett @{CHAIN_RATE} Hz -> TEYUN @{TAP_RATE} Hz: \
         {underruns} underrun(s), {xruns} xrun(s), peak load {load:.2}x the deadline over 10 s",
        match (heavy, mid_input) {
            (true, true) => "full preset + mid Input (4 pipelines)",
            (true, false) => "full preset",
            (false, true) => "one block + mid Input",
            (false, false) => "one block",
        }
    );
    (underruns, xruns)
}

#[test]
fn the_owners_two_interfaces_keep_the_tap_clean() {
    if !hw_tests_enabled("the_owners_two_interfaces_keep_the_tap_clean") {
        return;
    }
    let (underruns, xruns) = run(false, false);
    assert_eq!(
        underruns, 0,
        "#85: the tap starved {underruns} times in 10 s ({xruns} xrun(s)) between the \
         owner's two interfaces"
    );
}

/// His actual rig: the hard-rock preset feeding the tap across the two
/// interfaces.
#[test]
fn a_full_preset_across_the_owners_interfaces_stays_clean() {
    if !hw_tests_enabled("a_full_preset_across_the_owners_interfaces_stays_clean") {
        return;
    }
    let (underruns, xruns) = run(true, false);
    assert_eq!(
        underruns, 0,
        "#85: with a full preset the tap starved {underruns} times in 10 s \
         ({xruns} xrun(s)) — his '0 new xrun(s), 256 new underrun(s)'"
    );
}

/// His rig as it actually is: the hard-rock preset, a mid `Input` from the
/// interface's second channel AND the mid `Output` to the TEYUN — 2 inputs ×
/// 2 outputs, so FOUR independent pipelines each running the whole preset.
#[test]
fn his_four_pipeline_rig_keeps_the_tap_clean() {
    if !hw_tests_enabled("his_four_pipeline_rig_keeps_the_tap_clean") {
        return;
    }
    let (underruns, xruns) = run(true, true);
    assert_eq!(
        underruns, 0,
        "#85: his real shape starved {underruns} times in 10 s ({xruns} xrun(s)) — \
         four pipelines, each carrying the full preset"
    );
}
