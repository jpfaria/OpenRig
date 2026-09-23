//! Issue #969 — the owner switches scene (one chain off, the sibling on) and
//! the chain that should play comes up silent: it owns NO output route until
//! he toggles it off and on by hand. Measured live: four chains in the meters,
//! output routes for exactly one of them.
//!
//! Two chains share the loopback, each on its own capture channel (no #716
//! conflict), and the scene switch happens the way the GUI does it: one
//! `sync_project` that carries the new enabled flags. Every cycle asserts the
//! chain that just came on owns streams, owns output routes, and is audible —
//! and that the one that went off owns nothing.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_969_scene_switch_keeps_streams -- --nocapture
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
const CYCLES: usize = 4;

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
            .find(|d| {
                d.description()
                    .map(|desc| desc.name().contains(LOOPBACK))
                    .unwrap_or(false)
            })
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
                |e| eprintln!("[#969] loopback input error: {e}"),
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

fn settle(controller: &mut ProjectRuntimeController, chain_id: &ChainId, want_streams: bool) {
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        if (controller.stream_count(chain_id) > 0) == want_streams {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// How many output routes the chain's live runtime owns right now.
fn route_count(controller: &ProjectRuntimeController, chain_id: &ChainId) -> usize {
    controller
        .chain_runtime(chain_id)
        .map(|rt| rt.take_output_route_stats().len())
        .unwrap_or(0)
}

fn chain(id: &str, binding: &str, enabled: bool) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec![binding.into()],
        blocks: vec![first_preset_block()],
        di_output: None,
        loopers: vec![],
    }
}

fn binding(id: &str, in_channel: usize, loop_in: &str, loop_out: &str) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: vec![IoEndpoint {
            name: format!("in{in_channel}"),
            device_id: DeviceId(loop_in.into()),
            mode: ChannelMode::Mono,
            channels: vec![in_channel],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId(loop_out.into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

#[test]
fn switching_scene_leaves_the_enabled_chain_with_its_own_streams() {
    if !hw_tests_enabled("switching_scene_leaves_the_enabled_chain_with_its_own_streams") {
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
        .expect("#969 needs the BlackHole loopback INPUT");
    let loop_out = outputs
        .iter()
        .find(|d| d.name.contains(LOOPBACK))
        .expect("#969 needs the BlackHole loopback OUTPUT");

    let registry = vec![
        binding("scene-a", 0, &loop_in.id, &loop_out.id),
        binding("scene-b", 1, &loop_in.id, &loop_out.id),
    ];
    let a = ChainId("issue-969-a".into());
    let b = ChainId("issue-969-b".into());
    let mut project = Project {
        name: Some("issue-969".into()),
        device_settings: vec![settings(&loop_in.id), settings(&loop_out.id)],
        chains: vec![
            chain("issue-969-a", "scene-a", true),
            chain("issue-969-b", "scene-b", false),
        ],
        midi: None,
    };

    let listener = Listener::open();
    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start real streams");
    settle(&mut controller, &a, true);
    controller.set_chain_di_loop(&a, Some(di_tone()));
    let running = listener.measure(2);
    assert!(
        running > 0.01,
        "#969: the first scene was already silent before any switch (peak {running:.3})"
    );

    for cycle in 0..CYCLES {
        // The owner's action: ONE sync carrying the new scene — the chain that
        // was playing goes off, its sibling comes on.
        let (off, on) = if cycle % 2 == 0 {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
        for c in &mut project.chains {
            c.enabled = c.id == on;
        }
        controller.sync_project(&project).expect("sync scene");
        settle(&mut controller, &off, false);
        settle(&mut controller, &on, true);
        controller.set_chain_di_loop(&on, Some(di_tone()));

        let off_streams = controller.stream_count(&off);
        let on_streams = controller.stream_count(&on);
        let on_routes = route_count(&controller, &on);
        let peak = listener.measure(2);
        eprintln!(
            "[#969] cycle {cycle}: on={} streams={on_streams} routes={on_routes} peak={peak:.3} | off={} streams={off_streams}",
            on.0, off.0
        );

        assert_eq!(
            off_streams, 0,
            "#969 cycle {cycle}: the chain switched off still owns {off_streams} stream(s)"
        );
        assert!(
            on_streams > 0,
            "#969 cycle {cycle}: the chain switched on owns no stream"
        );
        assert!(
            on_routes > 0,
            "#969 cycle {cycle}: the chain switched on owns no OUTPUT ROUTE — \
             the live symptom (meters show the chain, openrig://routes shows nothing)"
        );
        assert!(
            peak > 0.01,
            "#969 cycle {cycle}: the scene that came on is silent (peak {peak:.3}) — \
             only a manual off/on brings it back"
        );
    }
}
