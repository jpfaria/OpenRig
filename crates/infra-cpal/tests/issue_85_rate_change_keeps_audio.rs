//! Issue #85 — changing a device's sample rate kills the audio until the app is
//! restarted ("o sistema não sai mais som depois que eu alterei a frequência").
//!
//! Same real-stream harness as the other #85 battery files: a DI loop is the
//! source, the chain writes to the BlackHole loopback, and the test listens on
//! BlackHole's input. The rig is measured at its first rate, the project's
//! device settings then move to another rate the device also supports, and the
//! rig must be audible again — a rate change is a rebuild, not a stop.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_85_rate_change_keeps_audio -- --nocapture
//! ```
#![cfg(target_os = "macos")]

mod hw_harness;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::AudioBlock;
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

const FIRST_RATE: u32 = 48_000;
const SECOND_RATE: u32 = 44_100;
const LOOPBACK: &str = "BlackHole";

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

fn di_tone(rate: u32) -> Arc<engine::DiPcm> {
    let frames = rate as usize;
    let step = 2.0 * std::f32::consts::PI * 440.0 / rate as f32;
    let samples: Vec<f32> = (0..frames).map(|i| 0.5 * (i as f32 * step).sin()).collect();
    Arc::new(engine::DiPcm::new(samples, rate, 1))
}

fn first_preset_block() -> AudioBlock {
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

struct Listener {
    peak_milli: Arc<AtomicU32>,
    _stream: cpal::Stream,
}

impl Listener {
    fn open() -> Self {
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
                |e| eprintln!("[#85 RATE] loopback input error: {e}"),
                None,
            )
            .expect("open loopback input stream");
        stream.play().expect("start loopback input");
        Self {
            peak_milli,
            _stream: stream,
        }
    }

    fn measure(&self, seconds: u64) -> f32 {
        self.peak_milli.store(0, Ordering::Relaxed);
        std::thread::sleep(Duration::from_secs(seconds));
        self.peak_milli.load(Ordering::Relaxed) as f32 / 1000.0
    }
}

fn settle(controller: &mut ProjectRuntimeController, chain_id: &ChainId) {
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        if controller.stream_count(chain_id) > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn changing_the_sample_rate_keeps_the_rig_audible() {
    if !hw_tests_enabled("changing_the_sample_rate_keeps_the_rig_audible") {
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

    let registry = vec![IoBinding {
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
            device_id: DeviceId(loop_out.id.clone()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];

    let chain_id = ChainId("issue-85-rate-change".into());
    let mut project = Project {
        name: Some("issue-85-rate-change".into()),
        device_settings: vec![
            settings(&loop_in.id, FIRST_RATE),
            settings(&loop_out.id, FIRST_RATE),
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["main".into()],
            blocks: vec![first_preset_block()],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };

    let listener = Listener::open();
    let mut controller =
        ProjectRuntimeController::start_with_io_bindings(&project, registry.clone())
            .expect("start real streams");
    settle(&mut controller, &chain_id);
    controller.set_chain_di_loop(&chain_id, Some(di_tone(FIRST_RATE)));

    let before = listener.measure(2);
    eprintln!("[#85 RATE] peak at {FIRST_RATE} Hz: {before:.3}");
    assert!(
        before > 0.01,
        "the rig was silent before the rate change (peak {before:.3})"
    );

    // The user's action: pick another sample rate in the audio settings.
    for setting in project.device_settings.iter_mut() {
        setting.sample_rate = SECOND_RATE;
    }
    controller
        .sync_project(&project)
        .expect("sync after the rate change");
    settle(&mut controller, &chain_id);
    // The GUI re-arms the DI after a rebuild; the loop is re-rendered per stream
    // rate, so the source is handed over again exactly as it is on that path.
    controller.set_chain_di_loop(&chain_id, Some(di_tone(SECOND_RATE)));

    let after = listener.measure(3);
    eprintln!("[#85 RATE] peak at {SECOND_RATE} Hz: {after:.3}");
    assert!(
        after > 0.01,
        "#85: the rig went silent after changing the sample rate \
         ({FIRST_RATE} → {SECOND_RATE} Hz): peak {after:.3} vs {before:.3} before. \
         A rate change is a rebuild, not a stop."
    );
}
