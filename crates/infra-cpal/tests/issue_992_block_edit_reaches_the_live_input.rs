//! Issue #992 — a block edit must reach the live input on real CoreAudio
//! streams.
//!
//! Reported 2026-09-29: the fil4 "has no effect on the live guitar" (history in
//! `docs/audio-incidents/992-filter-silent-on-live-guitar.md`). These run the
//! app's real activation and edit doors on real streams, a mono guitar input
//! into a mono Main, a stereo Main, and two guitars on one chain.
//!
//! Everything happens on the BlackHole loopback, split by channel so nothing
//! feeds back:
//!   * the test WRITES a 233 Hz tone on channel 0 — the guitar;
//!   * the chain's head input (a mono E/S, like `guitarra-1`) reads channel 0;
//!   * the chain's tail output writes channel 1, which the test LISTENS to.
//!
//! The edit goes through the controller the way `sync_live_chain_runtime`
//! delivers a parameter change (see #967).
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal \
//!     --test issue_992_block_edit_reaches_the_live_input -- --nocapture --test-threads=1
//! ```
#![cfg(target_os = "macos")]

mod hw_harness;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;

const RATE: u32 = 48_000;
const LOOPBACK: &str = "BlackHole";
/// A second virtual loopback for a stereo Main that must not feed the head.
const SECOND_LOOPBACK: &str = "MJAudioRecorder";
const SOURCE_CHANNEL: usize = 0;
const TAIL_CHANNEL: usize = 1;
const TONE_HZ: f32 = 233.0;
const CUT_DB: f32 = -18.0;
/// How far the measured drop may stray from the gain the block was given.
const TOLERANCE_DB: f32 = 2.0;

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

fn core(id: &str, effect_type: &str, model: &str, params: ParameterSet) -> AudioBlock {
    init_registry();
    let schema = schema_for_block_model(effect_type, model)
        .unwrap_or_else(|e| panic!("{model} schema must exist: {e}"));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.into(),
            model: model.into(),
            params: params
                .normalized_against(&schema)
                .unwrap_or_else(|e| panic!("{model} params must normalize: {e}")),
        }),
    }
}

fn fil4(gain_db: f32) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("gain", ParameterValue::Float(gain_db));
    core("issue992:fil4", "filter", "lv2_x42_fil4", params)
}

fn amp() -> AudioBlock {
    core(
        "issue992:amp",
        "preamp",
        "nam_marshall_jcm_800_2203_a2",
        ParameterSet::default(),
    )
}

fn cab() -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String("b".into()));
    core("issue992:cab", "cab", "ir_marshall_4x12_v30", params)
}

/// The owner's chains hold VST3 reverbs; a switched-off one builds as a
/// bypass node, so no plugin binary is needed.
fn vst3_off() -> AudioBlock {
    AudioBlock {
        id: BlockId("issue992:vst3-off".into()),
        enabled: false,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_VST3.into(),
            model: "vst3:Missing:Missing".into(),
            params: ParameterSet::default(),
        }),
    }
}

/// Plays the tone on `SOURCE_CHANNEL` of the loopback for as long as it lives.
fn play_source() -> cpal::Stream {
    let device = cpal::default_host()
        .output_devices()
        .expect("enumerate outputs")
        .find(|d| {
            d.description()
                .map(|desc| desc.name().contains(LOOPBACK))
                .unwrap_or(false)
        })
        .expect("loopback output device");
    let config = device.default_output_config().expect("loopback out config");
    let channels = config.config().channels as usize;
    let step = 2.0 * std::f32::consts::PI * TONE_HZ / config.config().sample_rate as f32;
    let mut phase = 0.0_f32;
    let stream = device
        .build_output_stream(
            &config.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for frame in data.chunks_mut(channels) {
                    let s = 0.25 * phase.sin();
                    phase = (phase + step) % (2.0 * std::f32::consts::PI);
                    for (ch, slot) in frame.iter_mut().enumerate() {
                        *slot = if ch == SOURCE_CHANNEL { s } else { 0.0 };
                    }
                }
            },
            |e| eprintln!("[#992] source output error: {e}"),
            None,
        )
        .expect("open loopback output stream");
    stream.play().expect("start source tone");
    stream
}

/// Listens to `TAIL_CHANNEL` of the loopback — where the chain's tail writes.
struct Listener {
    peak_micro: Arc<AtomicU32>,
    _stream: cpal::Stream,
}

impl Listener {
    fn open(device_name: &str) -> Self {
        let device = cpal::default_host()
            .input_devices()
            .expect("enumerate inputs")
            .find(|d| {
                d.description()
                    .map(|desc| desc.name().contains(device_name))
                    .unwrap_or(false)
            })
            .expect("listener input device");
        let config = device.default_input_config().expect("loopback config");
        let channels = config.config().channels as usize;
        let peak_micro = Arc::new(AtomicU32::new(0));
        let observed = Arc::clone(&peak_micro);
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
                    observed.fetch_max((peak * 1_000_000.0) as u32, Ordering::Relaxed);
                },
                |e| eprintln!("[#992] listener error: {e}"),
                None,
            )
            .expect("open loopback input stream");
        stream.play().expect("start listener");
        Self {
            peak_micro,
            _stream: stream,
        }
    }

    /// Peak on the tail channel over the next `span`, in dBFS.
    fn peak_db(&self, span: Duration) -> f32 {
        self.peak_micro.store(0, Ordering::Relaxed);
        std::thread::sleep(span);
        let peak = self.peak_micro.load(Ordering::Relaxed) as f32 / 1_000_000.0;
        20.0 * peak.max(1e-9).log10()
    }
}

struct LiveRig {
    controller: ProjectRuntimeController,
    project: Project,
    listener: Listener,
    _source: cpal::Stream,
    _device: hw_harness::DeviceFileLock,
}

/// Where the chain's tail goes: channel 1 of the loopback as a mono Main, or
/// a stereo Main `[0,1]` on a second loopback device (nothing feeds back).
#[derive(Clone, Copy)]
enum Main {
    Mono,
    Stereo,
    /// Two guitar E/S on one chain (In 1 and In 2), both into the stereo Main.
    TwoGuitarsStereo,
}

impl LiveRig {
    fn start(blocks: Vec<AudioBlock>) -> Self {
        Self::start_with(blocks, Main::Mono)
    }

    fn start_with(blocks: Vec<AudioBlock>, main: Main) -> Self {
        let _ = env_logger::try_init();
        let device = device_guard();
        init_registry();

        let inputs = list_input_device_descriptors().expect("list inputs");
        let outputs = list_output_device_descriptors().expect("list outputs");
        let loop_in = inputs
            .iter()
            .find(|d| d.name.contains(LOOPBACK))
            .expect("#992 needs the BlackHole loopback INPUT");
        let tail_device = match main {
            Main::Mono => LOOPBACK,
            Main::Stereo | Main::TwoGuitarsStereo => SECOND_LOOPBACK,
        };
        let loop_out = outputs
            .iter()
            .find(|d| d.name.contains(tail_device))
            .expect("#992 needs the tail's loopback OUTPUT");
        let mut registry = vec![IoBinding {
            id: "guitarra-1".into(),
            name: "guitarra-1".into(),
            inputs: vec![IoEndpoint {
                name: "in".into(),
                device_id: DeviceId(loop_in.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![SOURCE_CHANNEL],
            }],
            outputs: vec![match main {
                Main::Mono => IoEndpoint {
                    name: "main".into(),
                    device_id: DeviceId(loop_out.id.clone()),
                    mode: ChannelMode::Mono,
                    channels: vec![TAIL_CHANNEL],
                },
                Main::Stereo | Main::TwoGuitarsStereo => IoEndpoint {
                    name: "main".into(),
                    device_id: DeviceId(loop_out.id.clone()),
                    mode: ChannelMode::Stereo,
                    channels: vec![0, 1],
                },
            }],
        }];
        let mut bindings = vec!["guitarra-1".to_string()];
        if let Main::TwoGuitarsStereo = main {
            let mut second = registry[0].clone();
            second.id = "guitarra-2".into();
            second.name = "guitarra-2".into();
            second.inputs[0].channels = vec![TAIL_CHANNEL];
            registry.push(second);
            bindings.push("guitarra-2".into());
        }
        let chain_id = ChainId("issue-992-live-edit".into());
        let project = Project {
            name: Some("issue-992".into()),
            device_settings: vec![settings(&loop_in.id), settings(&loop_out.id)],
            chains: vec![Chain {
                id: chain_id.clone(),
                description: None,
                instrument: "electric_guitar".into(),
                enabled: true,
                volume: 100.0,
                io_binding_ids: bindings,
                blocks,
                di_output: None,
                loopers: vec![],
            }],
            midi: None,
        };

        let source = play_source();
        let listener = Listener::open(tail_device);
        let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
            .expect("start real streams");
        let deadline = Instant::now() + Duration::from_secs(10);
        while controller.stream_count(&chain_id) == 0 {
            assert!(
                Instant::now() < deadline,
                "no stream opened for the rig — the measurement would be meaningless"
            );
            controller.poll_pending_rebuilds();
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut rig = Self {
            controller,
            project,
            listener,
            _source: source,
            _device: device,
        };
        rig.tick(Duration::from_secs(2));
        rig
    }

    /// The frontend's tick for `span`.
    fn tick(&mut self, span: Duration) {
        let until = Instant::now() + span;
        while Instant::now() < until {
            self.controller.poll_pending_rebuilds();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn peak_db(&mut self) -> f32 {
        self.tick(Duration::from_millis(100));
        self.listener.peak_db(Duration::from_secs(2))
    }

    /// A parameter edit as `sync_live_chain_runtime` delivers it: the
    /// dispatcher has written the project, the controller gets the chain.
    fn edit(&mut self, blocks: Vec<AudioBlock>) {
        self.project.chains[0].blocks = blocks;
        let chain = self.project.chains[0].clone();
        let io_changed = self
            .controller
            .chain_io_changed(&self.project, &chain)
            .expect("io check");
        let rebound = self
            .controller
            .schedule_chain_activation(&self.project, &chain)
            .expect("schedule");
        let live = if rebound {
            false
        } else {
            self.controller
                .request_offthread_rebuild_if_live(&self.project, &chain)
                .expect("live rebuild")
        };
        eprintln!("[#992] edit: io_changed={io_changed} rebound={rebound} live_rebuild={live}");
        self.tick(Duration::from_secs(1));
    }
}

fn assert_cut(before: f32, after: f32, what: &str) {
    let drop = before - after;
    eprintln!("[#992] {what}: before {before:.1} dBFS, after {after:.1} dBFS, drop {drop:.1} dB");
    assert!(
        before > -40.0,
        "#992 {what}: the tone never reached the tail (peak {before:.1} dBFS)"
    );
    assert!(
        (drop - (-CUT_DB)).abs() <= TOLERANCE_DB,
        "#992 {what}: the fil4 master gain went to {CUT_DB} dB but the live input's \
         output dropped by {drop:.1} dB. A block edit must reach the live input."
    );
}

#[test]
fn turning_the_fil4_gain_down_cuts_the_live_input() {
    if !hw_tests_enabled("turning_the_fil4_gain_down_cuts_the_live_input") {
        return;
    }
    let mut rig = LiveRig::start(vec![fil4(0.0)]);
    let before = rig.peak_db();
    rig.edit(vec![fil4(CUT_DB)]);
    let after = rig.peak_db();
    assert_cut(before, after, "fil4 alone");
}

/// The owner's E/S shape: a mono guitar into a STEREO Main.
#[test]
fn turning_the_fil4_gain_down_cuts_the_live_input_into_a_stereo_main() {
    if !hw_tests_enabled("turning_the_fil4_gain_down_cuts_the_live_input_into_a_stereo_main") {
        return;
    }
    let mut rig = LiveRig::start_with(vec![fil4(0.0)], Main::Stereo);
    let before = rig.peak_db();
    rig.edit(vec![fil4(CUT_DB)]);
    let after = rig.peak_db();
    assert_cut(before, after, "fil4 alone, stereo Main");
}

/// The owner's DIGITAL chain: both guitars' E/S, a stereo Main.
#[test]
fn turning_the_fil4_gain_down_cuts_the_live_input_on_a_two_guitar_chain() {
    if !hw_tests_enabled("turning_the_fil4_gain_down_cuts_the_live_input_on_a_two_guitar_chain") {
        return;
    }
    let mut rig = LiveRig::start_with(vec![fil4(0.0)], Main::TwoGuitarsStereo);
    let before = rig.peak_db();
    rig.edit(vec![fil4(CUT_DB)]);
    let after = rig.peak_db();
    assert_cut(before, after, "fil4 alone, two guitars, stereo Main");
}

/// The owner's chain shape: amp, cab, the fil4 after them (linear there), a
/// VST3 switched off.
#[test]
fn turning_the_fil4_gain_down_on_the_owners_chain_cuts_the_live_input() {
    if !hw_tests_enabled("turning_the_fil4_gain_down_on_the_owners_chain_cuts_the_live_input") {
        return;
    }
    let chain = |gain| vec![amp(), cab(), fil4(gain), vst3_off()];
    let mut rig = LiveRig::start(chain(0.0));
    let before = rig.peak_db();
    rig.edit(chain(CUT_DB));
    let after = rig.peak_db();
    assert_cut(before, after, "amp + cab + fil4 + VST3 off");
}
