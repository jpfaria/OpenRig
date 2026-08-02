//! Issue #85 — a HEAVY rig (a full preset: NAM amp, IR cab, reverb…) with a mid
//! `Output` on another interface. The owner loaded his hard-rock preset and the
//! sound going to the TEYUN started failing; a single-block chain on the same
//! path is clean, so the question is what the real DSP load does to the tap's
//! route.
//!
//! Same measurement as the rest of the battery — DI loop as the source, the tap
//! writing to the BlackHole loopback, the test listening on its input — plus the
//! two counters that tell the two failure modes apart: **underruns** mean the
//! route was fed too slowly (routing/rate), **xruns** mean the callback missed
//! its deadline (CPU).
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_85_heavy_rig_mid_output -- --nocapture
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

const CHAIN_RATE: u32 = 44_100;
const TAP_RATE: u32 = 48_000;
const LOOPBACK: &str = "BlackHole";
const SILENT: &str = "MJAudioRecorder";

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

/// The WHOLE preset — the load a real rig carries.
fn heavy_blocks() -> Vec<AudioBlock> {
    let preset = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
    infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks
}

/// `mid` = measure the tap on the loopback; otherwise the chain's OWN tail is
/// the loopback, which is the control: it shows whether the preset itself lets
/// this DI tone through at all.
fn heavy_peak(mid: bool) -> (f32, u64, u64) {
    let _ = env_logger::try_init();
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let loop_in = inputs
        .iter()
        .find(|d| d.name.contains(LOOPBACK))
        .expect("#85 needs the BlackHole loopback INPUT");
    let loop_out = outputs
        .iter()
        .find(|d| d.name.contains(LOOPBACK))
        .expect("#85 needs the BlackHole loopback OUTPUT");
    let chain_out = outputs
        .iter()
        .find(|d| d.name.contains(SILENT))
        .expect("#85 needs a silent virtual output for the chain's own tail");

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
                device_id: DeviceId(if mid {
                    chain_out.id.clone()
                } else {
                    loop_out.id.clone()
                }),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
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

    // The mid Output sits at the END of the preset, the way the owner placed
    // his: after the cab, before the reverb tail of the chain's own output.
    let mut blocks = heavy_blocks();
    if mid {
        let mid_at = blocks.len().saturating_sub(1);
        blocks.insert(
            mid_at,
            AudioBlock {
                id: BlockId("issue85:mid-output".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: "aux".into(),
                    endpoint: "aux-out".into(),
                }),
            },
        );
        eprintln!(
            "[#85 HEAVY] {} blocks, mid Output at index {mid_at}",
            blocks.len()
        );
    }

    let chain_id = ChainId("issue-85-heavy".into());
    let project = Project {
        name: Some("issue-85-heavy".into()),
        device_settings: vec![
            settings(&loop_in.id, CHAIN_RATE),
            settings(&chain_out.id, CHAIN_RATE),
            settings(&loop_out.id, TAP_RATE),
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["main".into()],
            blocks,
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };

    let host = cpal::default_host();
    let device = host
        .input_devices()
        .expect("enumerate inputs")
        .find(|d| d.name().map(|n| n.contains(LOOPBACK)).unwrap_or(false))
        .expect("loopback input device");
    let config = device.default_input_config().expect("loopback config");
    let peak_milli = Arc::new(AtomicU32::new(0));
    let observed = Arc::clone(&peak_milli);
    let stream = device
        .build_input_stream(
            &config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut peak = 0.0_f32;
                for s in data {
                    peak = peak.max(s.abs());
                }
                observed.fetch_max((peak * 1000.0) as u32, Ordering::Relaxed);
            },
            |e| eprintln!("[#85 HEAVY] loopback input error: {e}"),
            None,
        )
        .expect("open loopback input stream");
    stream.play().expect("start loopback input");

    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start real streams");
    for _ in 0..200 {
        controller.poll_pending_rebuilds();
        if controller.stream_count(&chain_id) > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    controller.set_chain_di_loop(&chain_id, Some(di_tone()));
    std::thread::sleep(Duration::from_secs(3));

    let under0 = controller.chain_underrun_count(&chain_id);
    let xrun0 = controller.chain_xrun_count(&chain_id);
    peak_milli.store(0, Ordering::Relaxed);
    std::thread::sleep(Duration::from_secs(10));
    let underruns = controller.chain_underrun_count(&chain_id) - under0;
    let xruns = controller.chain_xrun_count(&chain_id) - xrun0;
    let peak = peak_milli.load(Ordering::Relaxed) as f32 / 1000.0;
    controller.set_chain_di_loop(&chain_id, None);
    drop(stream);

    eprintln!(
        "[#85 HEAVY] {} — peak {peak:.3}, {underruns} underrun(s), {xruns} xrun(s) over 10 s",
        if mid {
            "through the mid Output"
        } else {
            "as the chain's tail (control)"
        }
    );
    (peak, underruns, xruns)
}

/// Control: the full preset DOES pass this DI tone. If it does not, the test
/// below says nothing about the port.
#[test]
fn the_full_preset_passes_the_di_tone() {
    if !hw_tests_enabled("the_full_preset_passes_the_di_tone") {
        return;
    }
    let (peak, _, _) = heavy_peak(false);
    assert!(
        peak > 0.01,
        "the preset itself silences the DI tone (peak {peak:.3}) — fix the probe, \
         not the port"
    );
}

#[test]
fn a_heavy_rig_keeps_its_mid_output_clean() {
    if !hw_tests_enabled("a_heavy_rig_keeps_its_mid_output_clean") {
        return;
    }
    let (peak, underruns, xruns) = heavy_peak(true);
    assert!(
        peak > 0.01,
        "#85: the mid Output is SILENT under a full preset (peak {peak:.3}, \
         {underruns} underrun(s), {xruns} xrun(s)) — the owner's 'falhando o som \
         na saída para TEYUN' after loading his hard-rock preset"
    );
    assert_eq!(underruns, 0, "the mid Output starved under load");
}
