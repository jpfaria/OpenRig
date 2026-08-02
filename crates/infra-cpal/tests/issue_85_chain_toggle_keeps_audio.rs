//! Issue #85 — the user disabled and re-enabled the chain and the audio never
//! came back ("desativei e ativei a chain e parou de sair audio").
//!
//! Same real-stream harness as `issue_85_mid_output_reaches_its_device`: the DI
//! loop is the source, the chain writes to the BlackHole loopback, and the test
//! listens on BlackHole's input. Audio is measured THREE times — while running,
//! while disabled (must be silent), and after re-enabling (must be back).
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_85_chain_toggle_keeps_audio -- --nocapture
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

const RATE: u32 = 48_000;
const LOOPBACK: &str = "BlackHole";

fn settings(device_id: &str) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(device_id.into()),
        sample_rate: RATE,
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
    let frames = RATE as usize;
    let step = 2.0 * std::f32::consts::PI * 440.0 / RATE as f32;
    let samples: Vec<f32> = (0..frames).map(|i| 0.5 * (i as f32 * step).sin()).collect();
    Arc::new(engine::DiPcm::new(samples, RATE, 1))
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

/// Listens on the loopback input; `peak()` reads and resets the running peak.
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
                |e| eprintln!("[#85 TOGGLE] loopback input error: {e}"),
                None,
            )
            .expect("open loopback input stream");
        stream.play().expect("start loopback input");
        Self {
            peak_milli,
            _stream: stream,
        }
    }

    /// Peak observed over `seconds`, starting from silence.
    fn measure(&self, seconds: u64) -> f32 {
        self.peak_milli.store(0, Ordering::Relaxed);
        std::thread::sleep(Duration::from_secs(seconds));
        self.peak_milli.load(Ordering::Relaxed) as f32 / 1000.0
    }
}

/// Drains the control worker until the chain has streams (or the deadline).
fn settle(controller: &mut ProjectRuntimeController, chain_id: &ChainId, want_streams: bool) {
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        if (controller.stream_count(chain_id) > 0) == want_streams {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn disabling_and_re_enabling_a_chain_brings_the_audio_back() {
    if !hw_tests_enabled("disabling_and_re_enabling_a_chain_brings_the_audio_back") {
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

    let chain_id = ChainId("issue-85-toggle".into());
    let chain = Chain {
        id: chain_id.clone(),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![first_preset_block()],
        di_output: None,
        loopers: vec![],
    };
    let mut project = Project {
        name: Some("issue-85-toggle".into()),
        device_settings: vec![settings(&loop_in.id), settings(&loop_out.id)],
        chains: vec![chain],
        midi: None,
    };

    let listener = Listener::open();
    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start real streams");
    settle(&mut controller, &chain_id, true);
    controller.set_chain_di_loop(&chain_id, Some(di_tone()));

    let running = listener.measure(2);
    eprintln!("[#85 TOGGLE] peak while running: {running:.3}");
    assert!(
        running > 0.01,
        "the rig was already silent before the toggle (peak {running:.3})"
    );

    // The user's action: switch the chain off…
    project.chains[0].enabled = false;
    controller.sync_project(&project).expect("sync disabled");
    settle(&mut controller, &chain_id, false);
    let disabled = listener.measure(2);
    eprintln!("[#85 TOGGLE] peak while disabled: {disabled:.3}");
    assert!(
        disabled < 0.01,
        "a disabled chain must be silent (peak {disabled:.3})"
    );

    // …and back on. The DI loop is re-armed the way the GUI does after the
    // chain is rebuilt.
    project.chains[0].enabled = true;
    controller.sync_project(&project).expect("sync enabled");
    settle(&mut controller, &chain_id, true);
    controller.set_chain_di_loop(&chain_id, Some(di_tone()));
    let back = listener.measure(3);
    eprintln!("[#85 TOGGLE] peak after re-enabling: {back:.3}");
    assert!(
        back > 0.01,
        "#85: the audio never came back after re-enabling the chain \
         (peak {back:.3} vs {running:.3} while running)"
    );
}
