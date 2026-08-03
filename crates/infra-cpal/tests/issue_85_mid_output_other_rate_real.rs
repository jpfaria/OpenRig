//! Issue #85 — the mid `Output` on a device at ANOTHER sample rate, on real
//! CoreAudio streams: the owner's Scarlett at 44.1 kHz tapping a TEYUN whose
//! floor is 48 kHz. Before the per-route conversion this starved the tap's
//! elastic buffer ~8 % of the time: "saiu o som, mas bem ruim", 1024 underruns
//! per poll with zero xruns.
//!
//! Measured the same way as the rest of the #85 battery: a DI loop is the
//! source, the mid `Output` writes to the BlackHole loopback (opened at the
//! OTHER rate) and the test listens on BlackHole's input. Two things have to
//! hold at once — the tone arrives, and the chain reports no new underruns.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_85_mid_output_other_rate_real -- --nocapture
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
const CHAIN_RATE: u32 = 48_000;
/// The tap device's clock — deliberately different.
const TAP_RATE: u32 = 44_100;
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

#[test]
fn a_mid_output_on_another_clock_arrives_clean() {
    if !hw_tests_enabled("a_mid_output_on_another_clock_arrives_clean") {
        return;
    }
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
    // The chain's own I/O: the loopback clocks the input (the DI replaces the
    // samples) and a silent virtual device takes the tail, so the only thing
    // reaching the loopback OUTPUT is the tap.
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
                device_id: DeviceId(chain_out.id.clone()),
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
            blocks: vec![
                wire_block(),
                AudioBlock {
                    id: BlockId("issue85:mid-output".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".into(),
                        io: "aux".into(),
                        endpoint: "aux-out".into(),
                    }),
                },
            ],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };

    // Listen on the loopback input before anything writes to it.
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
            |e| eprintln!("[#85 RATE-REAL] loopback input error: {e}"),
            None,
        )
        .expect("open loopback input stream");
    stream.play().expect("start loopback input");

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
    peak_milli.store(0, Ordering::Relaxed);
    std::thread::sleep(Duration::from_secs(5));
    let underruns = controller.chain_underrun_count(&chain_id) - under0;
    let xruns = controller.chain_xrun_count(&chain_id) - xrun0;
    let peak = peak_milli.load(Ordering::Relaxed) as f32 / 1000.0;
    controller.set_chain_di_loop(&chain_id, None);
    drop(stream);

    eprintln!(
        "[#85 RATE-REAL] chain @{CHAIN_RATE} Hz → tap @{TAP_RATE} Hz: peak {peak:.3}, \
         {underruns} underrun(s), {xruns} xrun(s) over 5 s"
    );
    assert!(
        peak > 0.01,
        "nothing arrived on the tap's interface (peak {peak:.3})"
    );
    assert_eq!(
        underruns, 0,
        "#85: the tap starved {underruns} times in 5 s with {xruns} xrun(s) — a route \
         on another clock must be resampled, not fed at the wrong pace"
    );
}
