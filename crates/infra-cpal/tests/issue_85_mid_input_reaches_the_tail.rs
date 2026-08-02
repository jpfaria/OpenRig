//! Issue #85 — a mid `Input` port must bring ITS OWN E/S signal into the chain
//! at its position, on real CoreAudio streams.
//!
//! Everything happens on the BlackHole loopback, split by channel so nothing
//! feeds back:
//!   * the test WRITES a 440 Hz tone on channel 0 — that is the port's source;
//!   * the chain's head input reads channel 0 too, but its first block is a gate
//!     slammed shut, so nothing of it survives;
//!   * the chain's tail output writes channel 1, which the test LISTENS to.
//!
//! Three cases make the conclusion airtight: with the gate bypassed the tail
//! carries the tone (the path works); with the gate shut and no port the tail is
//! silent (the gate really kills it); with the gate shut and the mid `Input`
//! after it, the tone is back — it can only have entered through the port.
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_85_mid_input_reaches_the_tail -- --nocapture
//! ```
#![cfg(target_os = "macos")]

mod hw_harness;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InputBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;

const RATE: u32 = 48_000;
const LOOPBACK: &str = "BlackHole";
/// The test's tone sits here; the chain's tail writes the other channel.
const SOURCE_CHANNEL: usize = 0;
const TAIL_CHANNEL: usize = 1;

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

/// A gate that can never open: threshold at 100 % maps to 0 dBFS.
fn slammed_shut_gate(enabled: bool) -> AudioBlock {
    let schema = schema_for_block_model("dynamics", "gate_basic").expect("gate_basic schema");
    let mut ps = ParameterSet::default();
    ps.insert("threshold", ParameterValue::Float(100.0));
    ps.insert("attack_ms", ParameterValue::Float(0.1));
    ps.insert("release_ms", ParameterValue::Float(1.0));
    ps.insert("hold_ms", ParameterValue::Float(0.0));
    ps.insert("hysteresis_db", ParameterValue::Float(0.0));
    AudioBlock {
        id: BlockId("issue85:gate".into()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "dynamics".into(),
            model: "gate_basic".into(),
            params: ps.normalized_against(&schema).expect("gate params"),
        }),
    }
}

fn mid_input_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("issue85:mid-input".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-in".into(),
        }),
    }
}

/// Plays a 440 Hz tone on `SOURCE_CHANNEL` of the loopback for as long as it
/// lives — the signal the mid `Input` is supposed to pick up.
fn play_source(device_name: &str) -> cpal::Stream {
    let host = cpal::default_host();
    let device = host
        .output_devices()
        .expect("enumerate outputs")
        .find(|d| d.name().map(|n| n.contains(device_name)).unwrap_or(false))
        .expect("loopback output device");
    let config = device.default_output_config().expect("loopback out config");
    let channels = config.config().channels as usize;
    let step = 2.0 * std::f32::consts::PI * 440.0 / config.config().sample_rate as f32;
    let mut phase = 0.0_f32;
    let stream = device
        .build_output_stream(
            &config.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for frame in data.chunks_mut(channels) {
                    let s = 0.5 * phase.sin();
                    phase += step;
                    for (ch, slot) in frame.iter_mut().enumerate() {
                        *slot = if ch == SOURCE_CHANNEL { s } else { 0.0 };
                    }
                }
            },
            |e| eprintln!("[#85 MIDIN] source output error: {e}"),
            None,
        )
        .expect("open loopback output stream");
    stream.play().expect("start source tone");
    stream
}

/// Listens to `TAIL_CHANNEL` of the loopback — where the chain's tail writes.
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
        let channels = config.config().channels as usize;
        let peak_milli = Arc::new(AtomicU32::new(0));
        let observed = Arc::clone(&peak_milli);
        let stream = device
            .build_input_stream(
                &config.config(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut peak = 0.0_f32;
                    for frame in data.chunks(channels) {
                        if let Some(s) = frame.get(TAIL_CHANNEL) {
                            peak = peak.max(s.abs());
                        }
                    }
                    observed.fetch_max((peak * 1000.0) as u32, Ordering::Relaxed);
                },
                |e| eprintln!("[#85 MIDIN] listener error: {e}"),
                None,
            )
            .expect("open loopback input stream");
        stream.play().expect("start listener");
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

/// Runs the rig with `blocks` and returns the peak reaching the tail channel.
/// `head_on_loopback` picks where the chain's OWN input reads from: the
/// loopback channel that carries the tone (for the controls), or another
/// interface entirely (for the mid-port case, so the head can carry nothing and
/// the two taps never collide — #716 refuses to activate a chain whose input
/// device+channel is claimed twice).
fn tail_peak(blocks: Vec<AudioBlock>, head_on_loopback: bool) -> f32 {
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

    let head_in = if head_on_loopback {
        loop_in
    } else {
        inputs
            .iter()
            .find(|d| !d.name.contains(LOOPBACK) && !d.name.contains("Microfone"))
            .expect("#85 needs a second input interface for the chain's own E/S")
    };
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId(head_in.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![SOURCE_CHANNEL],
            }],
            outputs: vec![IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId(loop_out.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![TAIL_CHANNEL],
            }],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![IoEndpoint {
                name: "aux-in".into(),
                device_id: DeviceId(loop_in.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![SOURCE_CHANNEL],
            }],
            outputs: Vec::new(),
        },
    ];

    let chain_id = ChainId("issue-85-mid-input".into());
    let project = Project {
        name: Some("issue-85-mid-input".into()),
        device_settings: vec![
            settings(&head_in.id),
            settings(&loop_in.id),
            settings(&loop_out.id),
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

    let _source = play_source(LOOPBACK);
    let listener = Listener::open();
    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start real streams");
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        if controller.stream_count(&chain_id) > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        controller.stream_count(&chain_id) > 0,
        "no stream opened for the rig — the measurement would be meaningless"
    );
    // Let the stream settle before measuring: the first buffers after a cold
    // start pass through before the gate's envelope closes, and the peak is a
    // MAX — a startup transient would read as "the gate leaks".
    eprintln!(
        "[#85 MIDIN] streams for the chain: {}",
        controller.stream_count(&chain_id)
    );
    std::thread::sleep(Duration::from_secs(2));
    listener.measure(3)
}

/// Control 1: the chain is a wire, so the head input's tone reaches the tail.
#[test]
fn the_head_input_reaches_the_tail_when_the_gate_is_bypassed() {
    if !hw_tests_enabled("the_head_input_reaches_the_tail_when_the_gate_is_bypassed") {
        return;
    }
    let peak = tail_peak(vec![slammed_shut_gate(false)], true);
    eprintln!("[#85 MIDIN] peak with the gate bypassed: {peak:.3}");
    assert!(
        peak > 0.01,
        "the loopback path itself is silent (peak {peak:.3})"
    );
}

/// Control 2: with the gate shut and no port, nothing survives to the tail —
/// so any tone in the next test came through the port, not around it.
#[test]
fn the_shut_gate_silences_the_head_input() {
    if !hw_tests_enabled("the_shut_gate_silences_the_head_input") {
        return;
    }
    let peak = tail_peak(vec![slammed_shut_gate(true)], true);
    eprintln!("[#85 MIDIN] peak with the gate shut, no port: {peak:.3}");
    assert!(
        peak < 0.01,
        "the shut gate still let audio through (peak {peak:.3})"
    );
}

#[test]
fn a_mid_input_brings_its_signal_in_after_the_gate() {
    if !hw_tests_enabled("a_mid_input_brings_its_signal_in_after_the_gate") {
        return;
    }
    let peak = tail_peak(vec![slammed_shut_gate(true), mid_input_block()], false);
    eprintln!("[#85 MIDIN] peak with the mid Input after the gate: {peak:.3}");
    assert!(
        peak > 0.01,
        "#85: the mid Input's E/S never reached the tail (peak {peak:.3}) — a port \
         the user places mid-chain brings its own signal in at that point"
    );
}
