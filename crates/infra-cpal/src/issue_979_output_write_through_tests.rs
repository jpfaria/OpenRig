//! #979 — hypothesis: an output callback can return without writing the
//! whole device buffer it was handed. CoreAudio hands each output unit its IO
//! buffer holding whatever was last written to it, and cpal passes that buffer
//! straight to our callback. A path that leaves early and does not overwrite
//! every sample makes the HD 8 play the previous buffer again: a 64-frame
//! (1.45 ms) loop, repeated for as long as that path keeps returning early.
//! The early-return paths are:
//!
//! - the route missing from the runtime the stream holds;
//! - a runtime replaced or draining under the slot the stream captured;
//! - a lock the output side cannot take while the #670 worker holds it;
//! - the switched-off loop's send route, which the graph does not write;
//! - a device buffer of another size than the stream was built for.
//!
//! If the same path also skips the route's counters, `openrig://routes`
//! freezes.
//!
//! Measured live at 22:31, in the "Captured live" comment on the issue:
//! - the insert was off and there were two runtime groups;
//! - underruns reached 256/320 with 4 trims;
//! - then the counters froze at fill 0 while the sound kept looping ("loop" /
//!   stacked sound) until the chain was switched off and on.
//!
//! The decay spiral predicts counters that keep climbing, not freezing, and
//! `a_route_at_fill_zero_counts_every_short_callback` is green. Every harness
//! of this issue zeroes the output buffer before each callback. A callback
//! that writes nothing therefore reads as silence and can never fail a test.
//!
//! These tests pre-fill each callback's buffer (and its scratch) with a marker
//! no path can produce (+120 dBFS). They hold every output stream of the
//! owner's rig to two rules:
//! - on every callback, every sample of the device buffer is overwritten with
//!   audio or silence, never left as it was and never summed onto;
//! - every callback of a stream whose runtime writes its route advances that
//!   route's counters by exactly one.
//!
//! Rig: `rig:input-7` at 44.1 kHz / 64 frames on the Quantum HD 8 (30
//! channels). `guitarra-1` is on In 1 and `guitarra-2` on In 2, each E/S going
//! to Main `[0,1]` and Out `[10,11]`, plus the `syn2-main` loop with its send
//! on `[3]` and its return on `[2,3]`. Log: 1 input stream and 5 output
//! streams.
//!
//! No device is opened. The callbacks are driven through the slot seams the
//! real ones run (`process_input_buffer` / `process_output_buffer`), and the
//! control plane through the controller's own doors. The device id is a
//! stand-in that names no real interface.

#![cfg(test)]
#![cfg(not(all(target_os = "linux", feature = "jack")))]

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use engine::runtime::{process_output_f32, ChainRuntimeState, RuntimeGraph};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use crate::active_runtime::ActiveChainRuntime;
use crate::resolved::{ChainStreamSignature, InputStreamSignature, OutputStreamSignature};
use crate::{LiveRuntimeSlot, ProjectRuntimeController};

/// Stand-in id for the owner's Quantum HD 8. It names no real device.
const HD8: &str = "test:issue-979:quantum-hd-8";
/// The HD 8 exposes 30 channels each way to CoreAudio; every output stream of
/// the chain is opened with all of them.
const HD8_CHANNELS: usize = 30;
const RATE_HZ: u32 = 44_100;
const RATE: f32 = 44_100.0;
/// The owner's device buffer.
const FRAMES: usize = 64;
/// `elastic_targets` on macOS for every route of this chain: ×1 the buffer.
const TARGET: usize = 64;
/// Output routes of the two-head chain, in resolve order: Main and Out 2 of
/// `guitarra-1` (0, 1), the same of `guitarra-2` (2, 3), then the loop's send
/// (4). A bound loop owns its send whether it is on or off (#967).
const ROUTES: usize = 5;
/// Output routes of the chain with `guitarra-1` alone: Main, Out 2, the send.
const ONE_HEAD_ROUTES: usize = 3;
/// What the device buffer holds when a callback starts: +120 dBFS, a value no
/// callback path can produce.
const MARKER: f32 = 1.0e6;
/// The loudest sample this rig can hand the device: unity blocks, guitars at
/// −12 dBFS, the loop at unity. Anything louder is the marker, left in place,
/// scaled or summed onto.
const LEGIT: f32 = 64.0;
/// −60 dBFS: a stream playing a guitar.
const SIGNAL: f32 = 1e-3;
/// −80 dBFS: a stream playing nothing.
const SILENT: f32 = 1e-4;
/// Cycles that flush a ring, the loop and a rebuild fade (~140 ms).
const SETTLE: usize = 96;
/// Cycles the owner-visible counters are read over (~70 ms).
const MEASURE: usize = 48;
/// Switches per run.
const K: usize = 6;
/// Cycles of disturbed traffic per scene (~2.9 s).
const TRAFFIC_CYCLES: usize = 2_000;
/// Callbacks per device buffer size; the last 16 of each are listened to.
const SIZE_PHASE: usize = 160;
/// IO cycles raced against the workers (~4.4 s of audio).
const RACE_CYCLES: usize = 3_000;
/// Stale callbacks described in a failure; the rest are counted.
const REPORTED: usize = 12;

// ── The owner's rig ─────────────────────────────────────────────────────────

fn init_registry() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        block_gain::register_natives();
    });
}

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(HD8.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn guitar(id: &str, input: usize) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: vec![endpoint("guitar", ChannelMode::Mono, &[input])],
        outputs: vec![
            endpoint("Main L/R (Out 1/2)", ChannelMode::Stereo, &[0, 1]),
            endpoint("Out 2", ChannelMode::Stereo, &[10, 11]),
        ],
    }
}

/// The owner's `config.yaml` bindings this chain selects.
fn registry() -> Vec<IoBinding> {
    vec![
        guitar("guitarra-1", 0),
        guitar("guitarra-2", 1),
        IoBinding {
            id: "syn2-main".into(),
            name: "PEDAIS + SYN-2".into(),
            inputs: vec![endpoint("SYN-2 DI OUT L/R", ChannelMode::Stereo, &[2, 3])],
            outputs: vec![endpoint("pedais", ChannelMode::Mono, &[3])],
        },
    ]
}

/// A volume block at unity, standing in for the NAM preamp and the IR cab:
/// what is under test is what the callbacks hand the device, not the DSP.
fn volume_block(id: &str, enabled: bool) -> AudioBlock {
    init_registry();
    let schema = schema_for_block_model("gain", "volume").expect("volume schema must exist");
    let mut params = ParameterSet::default();
    params.insert("volume", ParameterValue::Float(100.0));
    AudioBlock {
        id: BlockId(id.into()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: params
                .normalized_against(&schema)
                .expect("the volume param must normalize"),
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Scene {
    insert: bool,
    preamp: bool,
}

/// `ANAL+DIG`, the scene of the 22:31 capture: loop off, preamp on.
const ANAL_DIG: Scene = Scene {
    insert: false,
    preamp: true,
};
const DRY: Scene = Scene {
    insert: false,
    preamp: false,
};
/// The scene of the report: the SYN-2 in the loop, preamp off.
const SYN2: Scene = Scene {
    insert: true,
    preamp: false,
};
const LOOP_PREAMP: Scene = Scene {
    insert: true,
    preamp: true,
};
/// Scene pairs a switch alternates between without moving the loop: the
/// grouping stays, so each switch is a DSP rebuild on the live streams (#967).
const SAME_LOOP_SCENES: [[Scene; 2]; 2] = [[ANAL_DIG, DRY], [SYN2, LOOP_PREAMP]];

fn chain_id() -> ChainId {
    ChainId("rig:input-7".into())
}

/// `rig:input-7` on the E/S in `heads`, in `scene`: the loop first, then the
/// preamp and the cab.
fn chain_with(heads: &[&str], scene: Scene) -> Chain {
    Chain {
        id: chain_id(),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: heads.iter().map(|head| head.to_string()).collect(),
        blocks: vec![
            AudioBlock {
                id: BlockId("syn2-main".into()),
                enabled: scene.insert,
                kind: AudioBlockKind::Insert(InsertBlock {
                    model: "standard".into(),
                    io: "syn2-main".into(),
                }),
            },
            volume_block("preamp", scene.preamp),
            volume_block("cab", true),
        ],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
    }
}

/// The owner's chain: both guitars.
fn chain_for(scene: Scene) -> Chain {
    chain_with(&["guitarra-1", "guitarra-2"], scene)
}

/// The same chain with `guitarra-1` alone: one runtime, so the loop's send
/// stream holds it even with the loop off (#967).
fn one_head_chain(scene: Scene) -> Chain {
    chain_with(&["guitarra-1"], scene)
}

/// A fresh build: what the chain switched off and on runs.
fn build(chain: &Chain, routes: usize) -> Vec<(usize, Arc<ChainRuntimeState>)> {
    init_registry();
    let targets = vec![TARGET; routes];
    engine::runtime::build_per_input_runtime_states(
        chain,
        RATE,
        &HashMap::new(),
        &targets,
        &registry(),
    )
    .expect("the owner's chain must build")
}

/// The route index of the loop's send, read off a build with the loop on.
fn send_route(chain_with_loop_on: &Chain, routes: usize) -> usize {
    build(chain_with_loop_on, routes)
        .iter()
        .flat_map(|(_, runtime)| runtime.take_output_route_stats())
        .find(|row| row.channels == [3])
        .map(|row| row.route)
        .expect("setup: with the loop on, one route writes the send [3]")
}

/// Route -> (frames queued, callbacks, underruns) of every route the runtime
/// writes (#947).
fn own_rows(runtime: &Arc<ChainRuntimeState>) -> BTreeMap<usize, (usize, u64, u64)> {
    runtime
        .take_output_route_stats()
        .into_iter()
        .map(|row| (row.route, (row.fill_frames, row.callbacks, row.underruns)))
        .collect()
}

fn runtime_of(slot: &LiveRuntimeSlot) -> Arc<ChainRuntimeState> {
    let guard = slot.load();
    let runtime: &Arc<ChainRuntimeState> = &guard;
    Arc::clone(runtime)
}

// ── The streams, as the stream builder binds them ───────────────────────────

/// What the device callbacks of the chain hold.
struct Streams {
    /// The HD 8's one input stream: every slot its callback fans out to, one
    /// #670 worker each.
    input: Vec<LiveRuntimeSlot>,
    /// One output stream per route, with the slots it mixes.
    outputs: Vec<(usize, Vec<LiveRuntimeSlot>)>,
}

/// Binds the streams to `slots` through the functions the stream builder
/// uses: `slots_for_input_stream` (#703) and `slots_for_output_stream` (#947).
fn plan_streams(slots: &[(usize, LiveRuntimeSlot)], chain: &Chain, routes: usize) -> Streams {
    let bindings = registry();
    let heads = engine::runtime_endpoints::resolve_chain_io(chain, &bindings).0;
    let map = crate::chain_resolve_io_map::output_devices_by_input_cpal(chain, &bindings, &heads);
    let input: Vec<LiveRuntimeSlot> = crate::slot_processing::slots_for_input_stream(slots, 0)
        .iter()
        .map(|slot| slot.handle())
        .collect();
    let outputs: Vec<(usize, Vec<LiveRuntimeSlot>)> = (0..routes)
        .map(|route| {
            let held = crate::slot_processing::slots_for_output_stream(slots, &map, HD8, route);
            let held: Vec<LiveRuntimeSlot> = held.iter().map(|slot| slot.handle()).collect();
            (route, held)
        })
        .collect();
    Streams { input, outputs }
}

/// The streams the live chain opened, bound to the controller's own slots:
/// the handles a cpal callback captures when its stream is built.
fn open_streams(controller: &ProjectRuntimeController, chain: &Chain) -> Streams {
    let mut slots: Vec<(usize, LiveRuntimeSlot)> = controller
        .chain_slots
        .iter()
        .filter(|((id, _), _)| id == &chain.id)
        .map(|((_, group), slot)| (*group, slot.handle()))
        .collect();
    slots.sort_by_key(|(group, _)| *group);
    plan_streams(&slots, chain, ROUTES)
}

// ── One output unit of the HD 8 ─────────────────────────────────────────────

/// What one callback left in the device buffer.
struct Scan {
    /// Samples the callback did not overwrite: still the marker, or the marker
    /// scaled or summed onto.
    stale: usize,
    /// Samples in the buffer.
    total: usize,
    /// The first such sample: (frame, channel, value).
    first: Option<(usize, usize, f32)>,
    /// The channels such samples sit on.
    channels: BTreeSet<usize>,
    /// Peak of the samples the callback did write.
    peak: f32,
}

impl Scan {
    fn describe(&self, route: usize, held: usize, when: &str) -> String {
        let (frame, channel, value) = self.first.unwrap_or((0, 0, 0.0));
        format!(
            "{when}: the output callback of route {route} ({held} runtime(s) held) left {} of \
             the {} samples of the device buffer as the device handed them (first at frame \
             {frame}, channel {channel}: {value}), on channels {:?} -- the HD 8 plays its \
             previous buffer again there, the owner's loop",
            self.stale, self.total, self.channels
        )
    }
}

fn scan(buffer: &[f32]) -> Scan {
    let mut result = Scan {
        stale: 0,
        total: buffer.len(),
        first: None,
        channels: BTreeSet::new(),
        peak: 0.0,
    };
    for (index, sample) in buffer.iter().enumerate() {
        if !sample.is_finite() || sample.abs() > LEGIT {
            result.stale += 1;
            result.channels.insert(index % HD8_CHANNELS);
            if result.first.is_none() {
                result.first = Some((index / HD8_CHANNELS, index % HD8_CHANNELS, *sample));
            }
        } else {
            result.peak = result.peak.max(sample.abs());
        }
    }
    result
}

/// One output stream of the chain, as its cpal callback runs it: the slots it
/// captured, the `loaded` and scratch buffers it keeps across callbacks, and
/// the device buffer CoreAudio hands it.
struct OutputUnit {
    route: usize,
    slots: Vec<LiveRuntimeSlot>,
    loaded: Vec<Arc<ChainRuntimeState>>,
    buffer: Vec<f32>,
    scratch: Vec<f32>,
    callbacks: u64,
}

impl OutputUnit {
    fn new(route: usize, slots: Vec<LiveRuntimeSlot>) -> Self {
        Self {
            route,
            slots,
            loaded: Vec::with_capacity(4),
            buffer: Vec::new(),
            scratch: Vec::new(),
            callbacks: 0,
        }
    }

    /// One device callback of `frames` frames. The buffer and the scratch
    /// both hold the marker when the callback starts.
    fn callback(&mut self, frames: usize) -> Scan {
        let len = frames * HD8_CHANNELS;
        self.buffer.clear();
        self.buffer.resize(len, MARKER);
        self.scratch.clear();
        self.scratch.resize(len, MARKER);
        crate::slot_processing::process_output_buffer(
            &self.slots,
            &mut self.loaded,
            self.route,
            &mut self.buffer,
            HD8_CHANNELS,
            &mut self.scratch,
        );
        self.callbacks += 1;
        scan(&self.buffer)
    }
}

/// Streams whose one held runtime writes their route, whose counter did not
/// move by exactly the callbacks the stream ran since `since` (route ->
/// callbacks already run then).
fn uncounted_callbacks(units: &[OutputUnit], since: &BTreeMap<usize, u64>) -> Vec<String> {
    let mut failures = Vec::new();
    for unit in units {
        let [slot] = unit.slots.as_slice() else {
            continue;
        };
        let runtime = runtime_of(slot);
        let Some(row) = runtime
            .take_output_route_stats()
            .into_iter()
            .find(|row| row.route == unit.route)
        else {
            continue;
        };
        let ran = unit.callbacks - since.get(&unit.route).copied().unwrap_or(0);
        if row.callbacks != ran {
            failures.push(format!(
                "route {}: its stream ran {ran} callbacks, the route counted {} -- {}",
                unit.route,
                row.callbacks,
                if row.callbacks < ran {
                    "callbacks that returned before counting, the frozen `openrig://routes`"
                } else {
                    "the route is counted by more than its own stream"
                }
            ));
        }
    }
    failures
}

// ── The HAL cycle ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum Play {
    Silence,
    Guitar1,
    Both,
}

#[derive(Clone, Copy)]
enum Worker {
    /// Drains its ring after the output callbacks of the cycle (#965).
    OnTime,
    /// Has not drained yet: the buffers wait in its ring.
    Late,
    /// A saturation recovery: re-promote and drop the backlog (#670).
    DropsBacklog,
}

/// A steady 220 Hz guitar at −12 dBFS.
fn tone(n: usize) -> f32 {
    0.25 * (2.0 * std::f32::consts::PI * 220.0 * (n % 44_100) as f32 / RATE).sin()
}

/// The HD 8 as #965 measured it: every unit on one IO thread, the input
/// callback first, every output callback right after, and the chain DSP on
/// the #670 workers after those. The SYN-2 loop is closed: what the send
/// played on channel 3 in one cycle comes back on In 3/4 in the next. Every
/// output callback starts with the marker in its buffer, and each one that
/// leaves any of it is recorded.
struct Hal {
    input: Vec<LiveRuntimeSlot>,
    units: Vec<OutputUnit>,
    /// The device buffer size, in frames.
    frames: usize,
    pedal: Vec<f32>,
    /// What the device sums from every unit this cycle (stale samples left out).
    device: Vec<f32>,
    /// Guitar sample clock.
    sample: usize,
    cycle: usize,
    /// Input buffers waiting in the workers' SPSC ring.
    ring: VecDeque<Vec<f32>>,
    /// What the rig is doing, for the failure lines.
    phase: String,
    /// The first stale callbacks, described.
    stale: Vec<String>,
    stale_callbacks: usize,
}

impl Hal {
    fn new(streams: Streams) -> Self {
        let Streams { input, outputs } = streams;
        Self {
            input,
            units: outputs
                .into_iter()
                .map(|(route, slots)| OutputUnit::new(route, slots))
                .collect(),
            frames: FRAMES,
            pedal: Vec::new(),
            device: Vec::new(),
            sample: 0,
            cycle: 0,
            ring: VecDeque::new(),
            phase: "steady".into(),
            stale: Vec::new(),
            stale_callbacks: 0,
        }
    }

    /// The input callback: the buffer goes to the workers' ring.
    fn begin(&mut self, play: Play) {
        let (guitar_1, guitar_2) = match play {
            Play::Silence => (false, false),
            Play::Guitar1 => (true, false),
            Play::Both => (true, true),
        };
        let mut input = vec![0.0_f32; self.frames * HD8_CHANNELS];
        for (i, frame) in input.chunks_mut(HD8_CHANNELS).enumerate() {
            let sample = tone(self.sample + i);
            let pedal = self.pedal.get(i).copied().unwrap_or(0.0);
            frame[0] = if guitar_1 { sample } else { 0.0 };
            frame[1] = if guitar_2 { sample } else { 0.0 };
            frame[2] = pedal;
            frame[3] = pedal;
        }
        self.sample += self.frames;
        self.ring.push_back(input);
        self.device.clear();
        self.device.resize(self.frames * HD8_CHANNELS, 0.0);
    }

    /// The output callback of unit `index`.
    fn output(&mut self, index: usize) -> (usize, Scan) {
        let Self {
            units,
            device,
            frames,
            cycle,
            phase,
            stale,
            stale_callbacks,
            ..
        } = self;
        let unit = &mut units[index];
        let result = unit.callback(*frames);
        for (sum, sample) in device.iter_mut().zip(unit.buffer.iter()) {
            if sample.is_finite() && sample.abs() <= LEGIT {
                *sum += *sample;
            }
        }
        if result.stale > 0 {
            *stale_callbacks += 1;
            if stale.len() < REPORTED {
                stale.push(result.describe(
                    unit.route,
                    unit.slots.len(),
                    &format!("cycle {cycle}, {phase}"),
                ));
            }
        }
        (unit.route, result)
    }

    /// Every unit's output callback, in route order.
    fn outputs(&mut self) -> Vec<(usize, Scan)> {
        let units = self.units.len();
        (0..units).map(|index| self.output(index)).collect()
    }

    /// The loop feeds back; the workers drain their ring, or not.
    fn end(&mut self, worker: Worker) {
        self.pedal = self
            .device
            .chunks(HD8_CHANNELS)
            .map(|frame| frame[3])
            .collect();
        match worker {
            Worker::OnTime => {
                while let Some(buffer) = self.ring.pop_front() {
                    for slot in &self.input {
                        crate::slot_processing::process_input_buffer(
                            slot,
                            0,
                            &buffer,
                            HD8_CHANNELS,
                        );
                    }
                }
            }
            Worker::Late => {}
            Worker::DropsBacklog => self.ring.clear(),
        }
        self.cycle += 1;
    }

    /// One HAL cycle. Returns what each output stream handed the device.
    fn cycle(&mut self, play: Play, worker: Worker) -> Vec<(usize, Scan)> {
        self.begin(play);
        let played = self.outputs();
        self.end(worker);
        played
    }

    fn run(&mut self, play: Play, cycles: usize) {
        for _ in 0..cycles {
            self.cycle(play, Worker::OnTime);
        }
    }

    /// Route -> callbacks its stream has run.
    fn callbacks_by_route(&self) -> BTreeMap<usize, u64> {
        self.units
            .iter()
            .map(|unit| (unit.route, unit.callbacks))
            .collect()
    }

    /// The stale callbacks, as failure lines.
    fn stale_report(&self, what: &str) -> Vec<String> {
        if self.stale_callbacks == 0 {
            return Vec::new();
        }
        let mut lines = vec![format!(
            "{what}: {} output callback(s) left part of the device buffer unwritten{}",
            self.stale_callbacks,
            if self.stale_callbacks > REPORTED {
                format!(" (the first {REPORTED} below)")
            } else {
                String::new()
            }
        )];
        lines.extend(self.stale.iter().cloned());
        lines
    }
}

// ── The controller, as an activation leaves it ──────────────────────────────

/// The chain live on the controller exactly as an activation leaves it: its
/// runtimes in the graph, one live slot per runtime (the slots every stream
/// captured), and an active entry whose stream signature is the one its
/// streams were opened for. No device is opened.
fn live_rig(scene: Scene) -> ProjectRuntimeController {
    let chain = chain_for(scene);
    let bindings = registry();
    let runtimes = build(&chain, ROUTES);
    let mut chains = HashMap::new();
    for (group, runtime) in &runtimes {
        chains.insert((chain.id.clone(), *group), Arc::clone(runtime));
    }
    let mut controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, RATE_HZ);
    controller.set_io_bindings(bindings.clone());
    controller.chain_slots.retain(|(id, _), _| id != &chain.id);
    for (group, runtime) in &runtimes {
        controller.chain_slots.insert(
            (chain.id.clone(), *group),
            LiveRuntimeSlot::new(Arc::clone(runtime)),
        );
    }
    let (inputs, outputs) = crate::io_topology::bound_io_signature(&chain, &bindings);
    controller.active_chains.insert(
        chain.id.clone(),
        ActiveChainRuntime {
            resolved: None,
            structure: Vec::new(),
            generation: 1,
            stream_signature: ChainStreamSignature {
                inputs: inputs
                    .into_iter()
                    .map(|(_, channels)| InputStreamSignature {
                        device_id: HD8.into(),
                        channels,
                        stream_channels: 30,
                        sample_rate: 44_100,
                        buffer_size_frames: 64,
                    })
                    .collect(),
                outputs: outputs
                    .into_iter()
                    .map(|(_, channels)| OutputStreamSignature {
                        device_id: HD8.into(),
                        channels,
                        stream_channels: 30,
                        sample_rate: 44_100,
                        buffer_size_frames: 64,
                    })
                    .collect(),
            },
            _input_streams: vec![],
            _output_streams: vec![],
        },
    );
    // Log of the live chain: 1 input stream and 5 output streams.
    controller.streams.streams_built(&chain.id, 1, 1, 5);
    controller
}

/// The app's path for a live edit (`sync_live_chain_runtime`): the switch is
/// handed to the off-thread rebuild door, and the install tick swaps it in
/// while the audio keeps running. Returns whether the door took it.
fn switch_live(controller: &mut ProjectRuntimeController, hal: &mut Hal, next: &Chain) -> bool {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![next.clone()],
        midi: None,
    };
    let took = controller
        .request_offthread_rebuild_if_live(&project, next)
        .expect("the live edit door must answer");
    if !took {
        return false;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut applied = 0;
    while applied == 0 || !controller.pending_rebuilds.is_empty() {
        hal.cycle(Play::Both, Worker::OnTime);
        applied += controller.poll_pending_rebuilds();
        assert!(Instant::now() < deadline, "the rebuild never landed");
        std::thread::sleep(Duration::from_micros(300));
    }
    true
}

/// (group, route) -> callbacks, as `openrig://routes` reads them.
fn owner_counters(controller: &ProjectRuntimeController) -> BTreeMap<(usize, usize), u64> {
    let mut counters = BTreeMap::new();
    for (group, rows) in controller.chain_output_route_stats(&chain_id()) {
        for row in rows {
            counters.insert((group, row.route), row.callbacks);
        }
    }
    counters
}

/// Publishes a fresh build into the slots the streams captured, group by
/// group, the way the install tick does (#672).
fn publish(slots: &[(usize, LiveRuntimeSlot)], runtimes: &[(usize, Arc<ChainRuntimeState>)]) {
    for (group, slot) in slots {
        let (_, runtime) = runtimes
            .iter()
            .find(|(fresh, _)| fresh == group)
            .expect("setup: a fresh build of the same scene has the same groups");
        let _replaced = slot.publish(Arc::clone(runtime));
    }
}

// ── Every path, on the owner's traffic ──────────────────────────────────────

/// The owner's traffic, cycle by cycle:
/// - a cold start, with the outputs running before the worker's first push
///   (the dev build underruns "from the first second");
/// - steady play, the guitars going quiet, and one guitar alone;
/// - one late buffer, and the worker 4 buffers behind;
/// - the worker 6 buffers behind, then a saturation recovery that drops the
///   backlog (#670).
fn traffic(cycle: usize) -> (Play, Worker, &'static str) {
    if cycle < 4 {
        return (
            Play::Both,
            Worker::Late,
            "cold start, the outputs running before the worker's first push",
        );
    }
    match (cycle % 400, cycle % 1_000) {
        (_, 500..=505) => (Play::Both, Worker::Late, "the worker 6 buffers behind"),
        (_, 506) => (
            Play::Both,
            Worker::DropsBacklog,
            "a saturation recovery dropping the backlog",
        ),
        (100, _) => (Play::Both, Worker::Late, "one late buffer"),
        (200..=203, _) => (Play::Both, Worker::Late, "the worker 4 buffers behind"),
        (300..=347, _) => (Play::Silence, Worker::OnTime, "the guitars silent"),
        (348..=399, _) => (Play::Guitar1, Worker::OnTime, "guitarra-1 alone"),
        _ => (Play::Both, Worker::OnTime, "steady"),
    }
}

/// Every output stream of the owner's rig, over the owner's traffic, with the
/// loop off (the 22:31 capture: two runtimes, and the send stream bound to
/// none) and on (one runtime writing all 5 routes). The traffic is a cold
/// start, underruns from late and dropped buffers, silence, and one guitar
/// alone.
///
/// Every callback must overwrite every sample of the 30-channel buffer it is
/// handed: its own channels with audio or silence, every other channel with
/// silence. Every stream whose runtime writes its route must count each of its
/// callbacks exactly once.
#[test]
fn every_output_callback_of_the_two_head_rig_writes_its_whole_device_buffer() {
    let mut failures = Vec::new();
    for scene in [ANAL_DIG, SYN2] {
        let state = if scene.insert { "loop on" } else { "loop off" };
        let chain = chain_for(scene);
        let runtimes = build(&chain, ROUTES);
        let slots = crate::build_chain_slots(&runtimes);
        let mut hal = Hal::new(plan_streams(&slots, &chain, ROUTES));
        for cycle in 0..TRAFFIC_CYCLES {
            let (play, worker, what) = traffic(cycle);
            hal.phase = format!("{state}, {what}");
            hal.cycle(play, worker);
        }
        failures.extend(hal.stale_report(state));
        failures.extend(
            uncounted_callbacks(&hal.units, &BTreeMap::new())
                .into_iter()
                .map(|line| format!("{state}: {line}")),
        );
    }
    assert!(
        failures.is_empty(),
        "an output callback of the owner's rig did not write its whole device buffer, or did \
         not count itself:\n{}",
        failures.join("\n")
    );
}

// ── Send route not in the graph ─────────────────────────────────────────────

/// The `syn2-main` loop is bound, so the chain opens a stream for its send
/// `[3]` whether the loop is on or off (#967). With the loop off the graph
/// writes nothing to that route. On the owner's two-head rig the builder
/// binds no runtime to the send stream. With one head the stream holds the
/// chain's runtime, whose send route is unwritten.
///
/// Either way the send's callback must hand the HD 8 silence in every sample
/// of its buffer, with the guitars playing, and never what the buffer held
/// before. The same holds for the engine call a held runtime makes on that
/// route, and for a send stream bound to nothing at all.
#[test]
fn a_switched_off_loops_send_stream_writes_silence_over_its_whole_buffer() {
    let mut failures = Vec::new();
    let rigs = [
        (
            "two heads (ANAL+DIG, the 22:31 capture)",
            chain_for(ANAL_DIG),
            chain_for(SYN2),
            ROUTES,
        ),
        (
            "one head",
            one_head_chain(ANAL_DIG),
            one_head_chain(SYN2),
            ONE_HEAD_ROUTES,
        ),
    ];
    for (name, chain, loop_on, routes) in &rigs {
        let send = send_route(loop_on, *routes);
        let runtimes = build(chain, *routes);
        let slots = crate::build_chain_slots(&runtimes);
        let mut hal = Hal::new(plan_streams(&slots, chain, *routes));
        let held = hal
            .units
            .iter()
            .find(|unit| unit.route == send)
            .map(|unit| unit.slots.len())
            .expect("setup: the chain opens a stream for its loop's send (#967)");

        hal.phase = format!("{name}, loop off, both guitars playing");
        let mut loudest = 0.0_f32;
        for _ in 0..2 * SETTLE {
            for (route, result) in hal.cycle(Play::Both, Worker::OnTime) {
                if route == send {
                    loudest = loudest.max(result.peak);
                }
            }
        }
        if loudest > SILENT {
            failures.push(format!(
                "{name}: the switched-off loop's send stream (route {send}, holding {held} \
                 runtime(s)) played {loudest:.5}: the guitar reached the SYN-2 with the loop off"
            ));
        }
        failures.extend(hal.stale_report(name));
        failures.extend(
            uncounted_callbacks(&hal.units, &BTreeMap::new())
                .into_iter()
                .map(|line| format!("{name}: {line}")),
        );

        for (group, runtime) in &runtimes {
            let mut out = vec![MARKER; FRAMES * HD8_CHANNELS];
            process_output_f32(runtime, send, &mut out, HD8_CHANNELS);
            let result = scan(&out);
            let when = format!(
                "{name}, group {group} popped on the send route its graph does not write \
                 (`process_output_f32`)"
            );
            if result.stale > 0 {
                failures.push(result.describe(send, 1, &when));
            }
            if result.peak > SILENT {
                failures.push(format!("{when}: it played {:.5}", result.peak));
            }
        }

        let mut unbound = OutputUnit::new(send, Vec::new());
        for call in 0..4 {
            let result = unbound.callback(FRAMES);
            let when = format!("{name}, call {call} of a send stream bound to no runtime");
            if result.stale > 0 {
                failures.push(result.describe(send, 0, &when));
            }
            if result.peak > SILENT {
                failures.push(format!("{when}: it played {:.5}", result.peak));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "the switched-off loop's send stream did not hand the HD 8 silence over its whole \
         buffer:\n{}",
        failures.join("\n")
    );
}

// ── Route missing ───────────────────────────────────────────────────────────

/// A switch that regroups the heads opens new streams, and the old streams
/// keep playing until the new ones are up (#881). Meanwhile an old stream
/// pops a runtime on a route that runtime does not write, or on a route index
/// the new chain does not have.
///
/// With the loop off (the 22:31 capture), each guitar's runtime is popped:
/// - on every route of the other E/S and on the send;
/// - on three route indices past the chain's.
///
/// Each pop goes through its stream and through the engine call. Each one must:
/// - hand the device silence in every sample, never what the buffer held;
/// - play nothing of its guitar on another E/S's output;
/// - not panic, since a panic kills the stream;
/// - leave the routes the runtime does write exactly as they were (fill,
///   callbacks, underruns).
#[test]
fn a_route_its_runtime_does_not_write_plays_silence_and_drains_nothing() {
    let chain = chain_for(ANAL_DIG);
    let runtimes = build(&chain, ROUTES);
    let slots = crate::build_chain_slots(&runtimes);
    let mut hal = Hal::new(plan_streams(&slots, &chain, ROUTES));
    hal.phase = "loop off, both guitars playing".into();
    hal.run(Play::Both, SETTLE);
    let mut failures = hal.stale_report("before the foreign pops");

    for (group, runtime) in &runtimes {
        let before = own_rows(runtime);
        let foreign: Vec<usize> = (0..ROUTES + 3)
            .filter(|route| !before.contains_key(route))
            .collect();
        for &route in &foreign {
            let mut unit = OutputUnit::new(route, vec![LiveRuntimeSlot::new(Arc::clone(runtime))]);
            for call in 0..4 {
                let when = format!(
                    "group {group} (it writes routes {:?}) popped on route {route} through its \
                     stream, call {call}",
                    before.keys().collect::<Vec<_>>()
                );
                match catch_unwind(AssertUnwindSafe(|| unit.callback(FRAMES))) {
                    Err(_) => failures.push(format!("{when}: the callback panicked")),
                    Ok(result) => {
                        if result.stale > 0 {
                            failures.push(result.describe(route, 1, &when));
                        }
                        if result.peak > SILENT {
                            failures.push(format!(
                                "{when}: it played {:.5}, its guitar on another E/S's output",
                                result.peak
                            ));
                        }
                    }
                }

                let when = format!(
                    "group {group} popped on route {route} by `process_output_f32`, call {call}"
                );
                let mut out = vec![MARKER; FRAMES * HD8_CHANNELS];
                let popped = catch_unwind(AssertUnwindSafe(|| {
                    process_output_f32(runtime, route, &mut out, HD8_CHANNELS);
                }));
                if popped.is_err() {
                    failures.push(format!("{when}: the callback panicked"));
                    continue;
                }
                let result = scan(&out);
                if result.stale > 0 {
                    failures.push(result.describe(route, 1, &when));
                }
                if result.peak > SILENT {
                    failures.push(format!(
                        "{when}: it played {:.5}, its guitar on another E/S's output",
                        result.peak
                    ));
                }
            }
        }
        let after = own_rows(runtime);
        if after != before {
            failures.push(format!(
                "group {group}: popping routes it does not write moved the routes it does, \
                 route -> (fill, callbacks, underruns) {before:?} -> {after:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "a route missing from the runtime a stream holds did not play clean silence:\n{}",
        failures.join("\n")
    );
}

// ── Runtime replaced ────────────────────────────────────────────────────────

/// Every stream reads its runtime through the `LiveRuntimeSlot` it captured,
/// and the 5 ms install publishes a rebuilt runtime into that slot while the
/// audio keeps running (#672, #967). It can land at any point of a HAL cycle.
///
/// The test covers, with the loop off and on:
/// - a publish between the input callback and the output callbacks;
/// - a publish between two output callbacks of the same cycle;
/// - then the live runtimes marked draining under their streams, as the
///   teardown does (#294).
///
/// Every callback after each of these must overwrite its whole buffer. From
/// the first callback after a publish, the new runtime's routes must count
/// every callback their streams run: those are the counters the owner reads.
#[test]
fn a_runtime_replaced_under_a_captured_slot_never_leaves_the_buffer_stale() {
    let mut failures = Vec::new();
    for scene in [ANAL_DIG, SYN2] {
        let state = if scene.insert { "loop on" } else { "loop off" };
        let chain = chain_for(scene);
        let slots = crate::build_chain_slots(&build(&chain, ROUTES));
        let mut hal = Hal::new(plan_streams(&slots, &chain, ROUTES));
        hal.phase = format!("{state}, before any swap");
        hal.run(Play::Both, SETTLE);

        let first = build(&chain, ROUTES);
        hal.begin(Play::Both);
        publish(&slots, &first);
        let since = hal.callbacks_by_route();
        hal.phase = format!("{state}, a rebuild published after the input callback");
        hal.outputs();
        hal.end(Worker::OnTime);
        hal.run(Play::Both, SETTLE);
        failures.extend(
            uncounted_callbacks(&hal.units, &since)
                .into_iter()
                .map(|line| format!("{state}, after a rebuild published mid-cycle: {line}")),
        );

        let second = build(&chain, ROUTES);
        hal.phase = format!("{state}, the cycle a rebuild is published between two outputs");
        hal.begin(Play::Both);
        hal.output(0);
        hal.output(1);
        publish(&slots, &second);
        let since = hal.callbacks_by_route();
        for index in 2..hal.units.len() {
            hal.output(index);
        }
        hal.end(Worker::OnTime);
        hal.phase = format!("{state}, after a rebuild published between two outputs");
        hal.run(Play::Both, SETTLE);
        failures.extend(
            uncounted_callbacks(&hal.units, &since)
                .into_iter()
                .map(|line| {
                    format!("{state}, after a rebuild published between two outputs: {line}")
                }),
        );

        for (_, runtime) in &second {
            runtime.set_draining();
        }
        hal.phase = format!("{state}, the live runtimes draining under their streams");
        hal.run(Play::Both, SETTLE);

        failures.extend(hal.stale_report(state));
    }
    assert!(
        failures.is_empty(),
        "a runtime replaced under the slot a stream captured left the device buffer stale or \
         its counters behind:\n{}",
        failures.join("\n")
    );
}

/// The owner reads `openrig://routes` every 0.5 s. The scene switches that
/// keep the loop where it was go through the app's own doors: the off-thread
/// rebuild, then the install tick, with the streams the chain opened still
/// playing (#967). This covers `ANAL+DIG` <-> dry with the loop off and
/// SYN-2 <-> SYN-2 + preamp with it on, K times each.
///
/// After every switch, over the next ~70 ms:
/// - the counters the owner reads must move by exactly the callbacks each
///   route's stream ran, never staying frozen while the stream plays;
/// - each route with a stream holding a runtime has one counter;
/// - no callback, during the switch or after it, leaves its buffer unwritten.
#[test]
fn the_route_counters_the_owner_reads_follow_every_callback_through_live_switches() {
    let mut failures = Vec::new();
    for scenes in SAME_LOOP_SCENES {
        let state = if scenes[0].insert {
            "loop on"
        } else {
            "loop off"
        };
        let mut controller = live_rig(scenes[0]);
        let mut hal = Hal::new(open_streams(&controller, &chain_for(scenes[0])));
        hal.phase = format!("{state}, as opened");
        hal.run(Play::Both, SETTLE);
        for k in 1..=K {
            let next = scenes[k % 2];
            hal.phase = format!("{state}, switch {k} -> {next:?} landing");
            assert!(
                switch_live(&mut controller, &mut hal, &chain_for(next)),
                "setup ({state}): switch {k} -> {next:?} keeps the loop where it was, so it is \
                 a DSP rebuild on the live streams (#967); the door refused it"
            );
            hal.phase = format!("{state}, after switch {k} -> {next:?}");
            let counted_before = owner_counters(&controller);
            let ran_before = hal.callbacks_by_route();
            hal.run(Play::Both, MEASURE);
            let counted_after = owner_counters(&controller);
            let ran_after = hal.callbacks_by_route();

            let streamed: BTreeSet<usize> = hal
                .units
                .iter()
                .filter(|unit| !unit.slots.is_empty())
                .map(|unit| unit.route)
                .collect();
            let counted: BTreeSet<usize> = counted_after.keys().map(|(_, route)| *route).collect();
            if counted != streamed {
                failures.push(format!(
                    "{state}, after switch {k}: the owner reads counters for routes \
                     {counted:?}, the output streams holding a runtime are {streamed:?}"
                ));
            }
            for ((group, route), after) in &counted_after {
                let before = counted_before.get(&(*group, *route)).copied().unwrap_or(0);
                let ran = ran_after.get(route).copied().unwrap_or(0)
                    - ran_before.get(route).copied().unwrap_or(0);
                match after.checked_sub(before) {
                    Some(moved) if moved == ran => {}
                    Some(moved) => failures.push(format!(
                        "{state}, after switch {k}: group {group} route {route} counted {moved} \
                         callbacks while its stream ran {ran}: {}",
                        if moved < ran {
                            "the counters the owner reads froze while the stream kept playing"
                        } else {
                            "the route is counted by more than its own stream"
                        }
                    )),
                    None => failures.push(format!(
                        "{state}, after switch {k}: group {group} route {route} went back from \
                         {before} to {after} callbacks with no switch in between"
                    )),
                }
            }
        }
        failures.extend(hal.stale_report(state));
    }
    assert!(
        failures.is_empty(),
        "through live scene switches, the routes the owner reads stopped following the \
         callbacks, or a callback left its buffer stale:\n{}",
        failures.join("\n")
    );
}

// ── Lock contended ──────────────────────────────────────────────────────────

/// Runs the rig's streams on real threads, as on the HD 8:
/// - one #670 worker thread per runtime, processing each buffer after the IO
///   thread's output callbacks of that cycle, and so while the next cycle's
///   output callbacks run (never more than 2 buffers behind);
/// - a control thread reading the routes the way `openrig://routes` does,
///   hammered;
/// - this thread as the IO thread.
///
/// Returns the failures.
fn race(scene: Scene) -> Vec<String> {
    let state = if scene.insert { "loop on" } else { "loop off" };
    let chain = chain_for(scene);
    let runtimes = build(&chain, ROUTES);
    let slots = crate::build_chain_slots(&runtimes);
    let Streams { input, outputs } = plan_streams(&slots, &chain, ROUTES);
    let mut units: Vec<OutputUnit> = outputs
        .into_iter()
        .map(|(route, held)| OutputUnit::new(route, held))
        .collect();

    let stop = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicUsize::new(0));
    let mut progress = Vec::new();
    let mut workers = Vec::new();
    for slot in &input {
        let slot = slot.handle();
        let stop = Arc::clone(&stop);
        let released = Arc::clone(&released);
        let done = Arc::new(AtomicUsize::new(0));
        progress.push(Arc::clone(&done));
        workers.push(std::thread::spawn(move || {
            let mut buffer = vec![0.0_f32; FRAMES * HD8_CHANNELS];
            let mut processed = 0usize;
            while !stop.load(Ordering::Acquire) {
                if processed >= released.load(Ordering::Acquire) {
                    std::thread::yield_now();
                    continue;
                }
                for (i, frame) in buffer.chunks_mut(HD8_CHANNELS).enumerate() {
                    let sample = tone(processed * FRAMES + i);
                    frame[0] = sample;
                    frame[1] = sample;
                }
                crate::slot_processing::process_input_buffer(&slot, 0, &buffer, HD8_CHANNELS);
                processed += 1;
                done.store(processed, Ordering::Release);
            }
        }));
    }
    let polled: Vec<Arc<ChainRuntimeState>> = runtimes
        .iter()
        .map(|(_, runtime)| Arc::clone(runtime))
        .collect();
    let stop_poll = Arc::clone(&stop);
    let poller = std::thread::spawn(move || {
        while !stop_poll.load(Ordering::Acquire) {
            for runtime in &polled {
                let _ = runtime.take_output_route_stats();
                let _ = runtime.underrun_count();
            }
            std::thread::yield_now();
        }
    });

    let mut failures = Vec::new();
    let mut stale_callbacks = 0usize;
    let deadline = Instant::now() + Duration::from_secs(60);
    'io: for cycle in 0..RACE_CYCLES {
        while progress
            .iter()
            .any(|done| done.load(Ordering::Acquire) + 2 < cycle)
        {
            if Instant::now() > deadline || workers.iter().any(|worker| worker.is_finished()) {
                failures.push(format!(
                    "{state}: setup: the workers stopped keeping up at cycle {cycle}"
                ));
                break 'io;
            }
            std::thread::yield_now();
        }
        for unit in units.iter_mut() {
            let result = unit.callback(FRAMES);
            if result.stale > 0 {
                stale_callbacks += 1;
                if stale_callbacks <= REPORTED {
                    failures.push(result.describe(
                        unit.route,
                        unit.slots.len(),
                        &format!("{state}, cycle {cycle}, its worker running at the same time"),
                    ));
                }
            }
        }
        released.store(cycle + 1, Ordering::Release);
    }
    stop.store(true, Ordering::Release);
    for worker in workers {
        if worker.join().is_err() {
            failures.push(format!("{state}: a #670 worker panicked"));
        }
    }
    if poller.join().is_err() {
        failures.push(format!("{state}: the route reader panicked"));
    }
    if stale_callbacks > REPORTED {
        failures.push(format!(
            "{state}: {stale_callbacks} stale callbacks in all (the first {REPORTED} above)"
        ));
    }
    failures.extend(
        uncounted_callbacks(&units, &BTreeMap::new())
            .into_iter()
            .map(|line| format!("{state}, with the workers and the reader racing: {line}")),
    );
    failures
}

/// On the rig, the #670 workers (~28% busy) process on their own RT threads
/// while the IO thread runs the output callbacks. The owner's route poll reads
/// the same runtimes from a third thread. So whatever the output side locks,
/// it finds held some of the time.
///
/// With the loop off (two workers) and on (one), racing for ~4.4 s of audio:
/// - no output callback may hand the device a buffer it did not write because
///   something was held;
/// - every callback must be counted on its route.
///
/// A callback that gave up on a held lock without writing replays the
/// previous buffer: the loop the owner hears, with counters that stop.
#[test]
fn an_output_callback_racing_its_workers_writes_every_frame_and_counts_itself() {
    let mut failures = Vec::new();
    for scene in [ANAL_DIG, SYN2] {
        failures.extend(race(scene));
    }
    assert!(
        failures.is_empty(),
        "an output callback running while its worker processed left the device buffer stale \
         or went uncounted:\n{}",
        failures.join("\n")
    );
}

// ── A device buffer of another size ─────────────────────────────────────────

/// The streams were built for the HD 8's 64-frame buffer. CoreAudio can hand
/// a callback a buffer of another size when the device's buffer is changed
/// under the app, for example by another app reopening the interface. The
/// test switches `ANAL+DIG` (loop off, both guitars playing) to 128-, 32- and
/// 256-frame buffers, then back to 64.
///
/// At every size:
/// - every callback must overwrite every sample it is handed;
/// - every callback must be counted on its route;
/// - in the last 16 of the 160 callbacks at that size, each route the graph
///   writes must carry its guitar.
///
/// A path that drops every buffer of an unexpected size leaves the counters,
/// fill included, frozen, and the device replays its last buffer. That is the
/// 22:31 end state: fill 0, counters stuck, the sound looping until off/on.
#[test]
fn a_device_buffer_of_another_size_is_written_whole_counted_and_heard() {
    let chain = chain_for(ANAL_DIG);
    let runtimes = build(&chain, ROUTES);
    let owned: BTreeSet<usize> = runtimes
        .iter()
        .flat_map(|(_, runtime)| own_rows(runtime).into_keys())
        .collect();
    let slots = crate::build_chain_slots(&runtimes);
    let mut hal = Hal::new(plan_streams(&slots, &chain, ROUTES));
    hal.phase = format!("{FRAMES}-frame buffers, as the streams were built");
    hal.run(Play::Both, SETTLE);

    let mut failures = Vec::new();
    for frames in [128, 32, 256, FRAMES] {
        hal.frames = frames;
        hal.phase =
            format!("the HD 8 handing {frames}-frame buffers to streams built for {FRAMES}");
        let mut peaks: BTreeMap<usize, f32> = BTreeMap::new();
        for cycle in 0..SIZE_PHASE {
            for (route, result) in hal.cycle(Play::Both, Worker::OnTime) {
                if cycle >= SIZE_PHASE - 16 {
                    let peak = peaks.entry(route).or_insert(0.0);
                    *peak = peak.max(result.peak);
                }
            }
        }
        for route in &owned {
            let peak = peaks.get(route).copied().unwrap_or(0.0);
            if peak < SIGNAL {
                failures.push(format!(
                    "{frames}-frame buffers: route {route} is silent ({peak:.5}) {} callbacks \
                     after the change, with its guitar playing: the stream stopped carrying it",
                    SIZE_PHASE - 16
                ));
            }
        }
    }
    failures.extend(hal.stale_report("buffer size changes"));
    failures.extend(uncounted_callbacks(&hal.units, &BTreeMap::new()));
    assert!(
        failures.is_empty(),
        "a device buffer of another size than the streams were built for was not written \
         whole, counted and played:\n{}",
        failures.join("\n")
    );
}
