//! #979 — hypothesis: a device notification that changes nothing rebuilds the
//! live chain. Measured on the owner's rig: the chain was rebuilt 3 times in 40
//! minutes (route callbacks reset at 13.1, 18.1 and 35.5 min) with nobody
//! touching it, and the send route `[3]` came and went with those rebuilds.
//! Round 2 pinned that the app's own ticks, route reads and worker stalls do
//! not do it
//! (`an_unchanged_chain_is_never_rebuilt_by_ticks_reads_or_worker_events` is
//! green), so the trigger has to come from outside the app:
//! CoreAudio saying the device list changed (another interface or another
//! app's aggregate device came or went, the system default moved), a property
//! of the Quantum HD 8 changed to the value it already had (another app
//! reopening it at the same 44.1 kHz / 64 frames, coreaudiod restarting its
//! IO), or an overload and the #670 saturation recovery after it.
//!
//! None of those is an edit. The owner's invariants: nothing but an edit may
//! rebuild a live chain; N streams = N isolated pipelines, so a change to
//! another device or another E/S never touches this chain; zero dropouts;
//! latency never grows. After any such notification the chain must be the one
//! that was playing: the same runtime identity, the same runtimes in the graph
//! and in the live slots, the same streams (generation and counts), nothing
//! installed or on its way, route counters that never go back, and every
//! stream heard exactly as it is heard with no notification at all.
//!
//! The notification goes through the doors the app runs for it: the device
//! cache is dropped (`invalidate_device_cache`, #693/#829), the E/S registry
//! re-derived from the new list is pushed into the runtime (`set_io_bindings`,
//! `SetIoBindings`, #716/#127), the live-edit checks the drain runs on a
//! re-sync (`chain_io_changed`, `schedule_chain_activation`) are asked whether
//! the chain moved, and the 2 s health tick asks `is_healthy` before it
//! reconnects, which tears down and reopens every stream (#913).
//!
//! Topology: `rig:input-7` at 44.1 kHz / 64 frames on the HD 8, `guitarra-1`
//! (In 1) and `guitarra-2` (In 2), each E/S to Main `[0,1]` and Out `[10,11]`,
//! the `syn2-main` loop with its send on `[3]` and its return on `[2,3]`, plus
//! the machine's `default` E/S on another device, the way #716 creates it.
//! Every device id is a stand-in: nothing here can open or even find a real
//! interface, and no stream is opened. The callbacks are driven through the
//! slot seams the real ones run, the way
//! `issue_979_stream_layer_replaced_runtime_tests` does.

#![cfg(test)]
#![cfg(not(all(target_os = "linux", feature = "jack")))]

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use engine::runtime::{ChainRuntimeState, RuntimeGraph};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use crate::active_runtime::ActiveChainRuntime;
use crate::resolved::{ChainStreamSignature, InputStreamSignature, OutputStreamSignature};
use crate::{LiveRuntimeSlot, ProjectRuntimeController};

/// Stand-in id for the owner's Quantum HD 8. It names no real device.
const HD8: &str = "test:issue-979:quantum-hd-8";
/// Stand-in for the Mac's built-in output, where #716 puts the `default` E/S.
const BUILT_IN: &str = "test:issue-979:built-in";
/// Stand-in for an aggregate device another app creates (a meeting app, a
/// streaming app): it appears in the device list and takes the system default.
const AGGREGATE: &str = "test:issue-979:another-apps-aggregate";
/// Stand-in for a USB headset plugged in while the owner plays.
const HEADSET: &str = "test:issue-979:usb-headset";
/// The HD 8 exposes 30 channels each way to CoreAudio.
const HD8_CHANNELS: usize = 30;
const RATE_HZ: u32 = 44_100;
const RATE: f32 = 44_100.0;
/// The owner's device buffer.
const FRAMES: usize = 64;
/// `elastic_targets` on macOS for every route of this chain: x1 the buffer.
const TARGET: usize = 64;
/// Output endpoints the chain opens a stream for: Main and Out 2 of
/// `guitarra-1`, the same of `guitarra-2`, then the loop's send. A bound loop
/// owns its send whether it is on or off (#967).
const ROUTES: usize = 5;
/// The 5 ms `rebuild_install_timer` (#967), in 64-frame cycles.
const INSTALL_TICK: usize = 3;
/// Two seconds of cycles: the warm-up, and the audio-health tick (#913).
const TWO_SECONDS: usize = 1_378;
/// The owner's `openrig://routes` poll and the meter tick.
const HALF_SECOND: usize = 344;
/// Cycles that flush a ring and a rebuild fade (~140 ms).
const SETTLE: usize = 96;
/// Notification rounds per run.
const K: usize = 6;
/// Two rigs fed the same input sound the same to this tolerance.
const EPS: f32 = 1e-6;

// ── The owner's rig ─────────────────────────────────────────────────────────

fn init_registry() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        block_gain::register_natives();
    });
}

fn endpoint_on(device: &str, name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn guitar(id: &str, input: usize) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: vec![endpoint_on(HD8, "guitar", ChannelMode::Mono, &[input])],
        outputs: vec![
            endpoint_on(HD8, "Main L/R (Out 1/2)", ChannelMode::Stereo, &[0, 1]),
            endpoint_on(HD8, "Out 2", ChannelMode::Stereo, &[10, 11]),
        ],
    }
}

/// The E/S this chain selects, from the owner's `config.yaml`.
fn registry() -> Vec<IoBinding> {
    vec![
        guitar("guitarra-1", 0),
        guitar("guitarra-2", 1),
        IoBinding {
            id: "syn2-main".into(),
            name: "PEDAIS + SYN-2".into(),
            inputs: vec![endpoint_on(
                HD8,
                "SYN-2 DI OUT L/R",
                ChannelMode::Stereo,
                &[2, 3],
            )],
            outputs: vec![endpoint_on(HD8, "pedais", ChannelMode::Mono, &[3])],
        },
    ]
}

/// The machine's `default` E/S, which the audio wizard and the device refresh
/// keep on the system default device (#716). `rig:input-7` never selects it.
fn default_io(device: &str) -> IoBinding {
    IoBinding {
        id: "default".into(),
        name: "DEFAULT".into(),
        inputs: vec![endpoint_on(device, "In 1", ChannelMode::Mono, &[0])],
        outputs: vec![endpoint_on(device, "Out 1/2", ChannelMode::Stereo, &[0, 1])],
    }
}

/// The whole registry the runtime holds: the chain's three E/S plus the
/// `default` one wherever the system default is (`None`: dropped).
fn machine_registry(default_device: Option<&str>) -> Vec<IoBinding> {
    let mut bindings = registry();
    if let Some(device) = default_device {
        bindings.push(default_io(device));
    }
    bindings
}

/// A volume block at unity, standing in for the NAM preamp and the IR cab.
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

/// `ANAL+DIG`, the scene of the live capture: loop off, preamp on, two runtimes.
const ANAL_DIG: Scene = Scene {
    insert: false,
    preamp: true,
};
const DRY: Scene = Scene {
    insert: false,
    preamp: false,
};
/// The scene of the report: the SYN-2 in the loop, preamp off, one runtime.
const SYN2: Scene = Scene {
    insert: true,
    preamp: false,
};
const LOOP_PREAMP: Scene = Scene {
    insert: true,
    preamp: true,
};
/// Scene pairs a footswitch alternates between without moving the loop: each
/// switch is a DSP rebuild on the live streams (#967).
const SAME_LOOP_SCENES: [[Scene; 2]; 2] = [[ANAL_DIG, DRY], [SYN2, LOOP_PREAMP]];

fn state_of(scene: Scene) -> &'static str {
    if scene.insert {
        "loop on (SYN2, one runtime)"
    } else {
        "loop off (ANAL+DIG, two runtimes)"
    }
}

fn chain_id() -> ChainId {
    ChainId("rig:input-7".into())
}

/// `rig:input-7` in `scene`: the loop first, then the preamp and the cab.
fn chain_for(scene: Scene) -> Chain {
    Chain {
        id: chain_id(),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into(), "guitarra-2".into()],
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
        mix: Default::default(),
    }
}

/// The project the app re-syncs from. No device settings, as in round 2: the
/// live device config serves every build (#957).
fn project_for(chain: &Chain) -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain.clone()],
        midi: None,
    }
}

/// A fresh build of `scene`.
fn fresh_runtimes(scene: Scene) -> Vec<(usize, Arc<ChainRuntimeState>)> {
    init_registry();
    engine::runtime::build_per_input_runtime_states(
        &chain_for(scene),
        RATE,
        &HashMap::new(),
        &[TARGET; ROUTES],
        &registry(),
    )
    .expect("the owner's chain must build")
}

/// The chain live on the controller exactly as an activation leaves it: its
/// runtimes in the graph, one live slot per runtime (the slots every stream
/// captured), and an active entry holding the stream signature its streams
/// were opened for and the structure signature the activation stored. No
/// device is opened.
fn live_rig(scene: Scene, bindings: Vec<IoBinding>) -> ProjectRuntimeController {
    let chain = chain_for(scene);
    let runtimes = fresh_runtimes(scene);
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
            structure: crate::io_topology::chain_structure_signature(&chain, &bindings),
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
fn plan_streams(slots: &[(usize, LiveRuntimeSlot)], chain: &Chain) -> Streams {
    let bindings = registry();
    let heads = engine::runtime_endpoints::resolve_chain_io(chain, &bindings).0;
    let map = crate::chain_resolve_io_map::output_devices_by_input_cpal(chain, &bindings, &heads);
    let input: Vec<LiveRuntimeSlot> = crate::slot_processing::slots_for_input_stream(slots, 0)
        .iter()
        .map(|slot| slot.handle())
        .collect();
    let outputs: Vec<(usize, Vec<LiveRuntimeSlot>)> = (0..ROUTES)
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
    plan_streams(&slots, chain)
}

fn runtime_of(slot: &LiveRuntimeSlot) -> Arc<ChainRuntimeState> {
    let guard = slot.load();
    let runtime: &Arc<ChainRuntimeState> = &guard;
    Arc::clone(runtime)
}

fn ptr(runtime: &Arc<ChainRuntimeState>) -> usize {
    Arc::as_ptr(runtime) as usize
}

/// Every runtime of the chain the controller's graph holds, by group.
fn live(controller: &ProjectRuntimeController) -> Vec<(usize, Arc<ChainRuntimeState>)> {
    let mut live = controller
        .runtime_graph
        .runtimes_with_groups_for(&chain_id());
    live.sort_by_key(|(group, _)| *group);
    live
}

fn graph_ptrs(controller: &ProjectRuntimeController) -> BTreeMap<usize, usize> {
    live(controller)
        .iter()
        .map(|(group, runtime)| (*group, ptr(runtime)))
        .collect()
}

/// Group -> the runtime its live slot publishes to the callbacks.
fn slot_ptrs(controller: &ProjectRuntimeController) -> BTreeMap<usize, usize> {
    controller
        .chain_slots
        .iter()
        .filter(|((id, _), _)| id == &chain_id())
        .map(|((_, group), slot)| (*group, ptr(&runtime_of(slot))))
        .collect()
}

/// The routes a runtime owns: those its own pipelines write (#947).
fn owned_routes(runtime: &Arc<ChainRuntimeState>) -> BTreeSet<usize> {
    runtime
        .take_output_route_stats()
        .into_iter()
        .map(|row| row.route)
        .collect()
}

/// How a runtime reads in a failure: its group while the graph holds it.
fn names(ptrs: &[usize], live: &[(usize, Arc<ChainRuntimeState>)]) -> Vec<String> {
    ptrs.iter()
        .map(|p| {
            live.iter()
                .find(|(_, runtime)| ptr(runtime) == *p)
                .map(|(group, _)| format!("group {group}"))
                .unwrap_or_else(|| "a runtime the graph no longer holds".to_string())
        })
        .collect()
}

/// Where the captured streams disagree with the runtimes the graph holds: a
/// worker on another runtime, a stream that pops a runtime that does not own
/// its route, the live slots out of lock-step with the graph (#672).
fn stream_layer_mismatches(
    controller: &ProjectRuntimeController,
    streams: &Streams,
) -> Vec<String> {
    let mut failures = Vec::new();
    let now = live(controller);
    let (graph, slots) = (graph_ptrs(controller), slot_ptrs(controller));
    if slots != graph {
        failures.push(format!(
            "the live slots publish {:?} (group: runtime), the graph holds {:?}",
            slots, graph
        ));
    }
    let mut expected: Vec<usize> = now.iter().map(|(_, runtime)| ptr(runtime)).collect();
    expected.sort_unstable();
    let mut fed: Vec<usize> = streams
        .input
        .iter()
        .map(|slot| ptr(&runtime_of(slot)))
        .collect();
    fed.sort_unstable();
    if fed != expected {
        failures.push(format!(
            "the input stream's #670 workers push into {:?}, the graph holds {:?}",
            names(&fed, &now),
            names(&expected, &now)
        ));
    }
    for (route, held) in &streams.outputs {
        let mut holders: Vec<usize> = held.iter().map(|slot| ptr(&runtime_of(slot))).collect();
        holders.sort_unstable();
        let mut owners: Vec<usize> = now
            .iter()
            .filter(|(_, runtime)| owned_routes(runtime).contains(route))
            .map(|(_, runtime)| ptr(runtime))
            .collect();
        owners.sort_unstable();
        if holders != owners || owners.len() > 1 {
            failures.push(format!(
                "the output stream of route {route} pops {:?}, the route is owned by {:?} (#947)",
                names(&holders, &now),
                names(&owners, &now)
            ));
        }
    }
    failures
}

// ── The HAL cycle, through the streams ──────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Worker {
    /// Drains its ring after the output callbacks of the cycle (#965).
    OnTime,
    /// Has not drained yet: the buffers wait in its ring.
    Late,
    /// A saturation recovery: re-promote and drop the backlog (#670).
    DropsBacklog,
}

/// A steady 220 Hz guitar at -12 dBFS.
fn tone(n: usize) -> f32 {
    0.25 * (2.0 * std::f32::consts::PI * 220.0 * (n % 44_100) as f32 / RATE).sin()
}

/// The HD 8 as #965 measured it: every unit on one IO thread, the input
/// callback first, every output callback right after, and the chain DSP on
/// the #670 workers after those. Both guitars play. The SYN-2 loop is closed:
/// what the send played on channel 3 in one cycle comes back on In 3/4 in the
/// next. A cycle the device does not run (its IO stopped while another app
/// reconfigures it) is simply a cycle nobody calls.
struct Hal {
    streams: Streams,
    pedal: Vec<f32>,
    clock: usize,
    /// Input buffers waiting in the workers' SPSC ring.
    ring: VecDeque<Vec<f32>>,
}

impl Hal {
    fn new(streams: Streams) -> Self {
        Self {
            streams,
            pedal: vec![0.0; FRAMES],
            clock: 0,
            ring: VecDeque::new(),
        }
    }

    /// One HAL cycle. Returns what each output stream handed the device.
    fn cycle(&mut self, worker: Worker) -> Vec<(usize, Vec<f32>)> {
        let mut input = vec![0.0_f32; FRAMES * HD8_CHANNELS];
        for (i, frame) in input.chunks_mut(HD8_CHANNELS).enumerate() {
            let sample = tone(self.clock * FRAMES + i);
            frame[0] = sample;
            frame[1] = sample;
            frame[2] = self.pedal[i];
            frame[3] = self.pedal[i];
        }
        self.clock += 1;
        self.ring.push_back(input);

        let mut device = vec![0.0_f32; FRAMES * HD8_CHANNELS];
        let mut played = Vec::with_capacity(self.streams.outputs.len());
        for (route, slots) in &self.streams.outputs {
            let mut out = vec![0.0_f32; FRAMES * HD8_CHANNELS];
            let mut scratch = vec![0.0_f32; FRAMES * HD8_CHANNELS];
            let mut loaded: Vec<Arc<ChainRuntimeState>> = Vec::with_capacity(4);
            crate::slot_processing::process_output_buffer(
                slots,
                &mut loaded,
                *route,
                &mut out,
                HD8_CHANNELS,
                &mut scratch,
            );
            for (sum, sample) in device.iter_mut().zip(&out) {
                *sum += *sample;
            }
            played.push((*route, out));
        }
        for (pedal, frame) in self.pedal.iter_mut().zip(device.chunks(HD8_CHANNELS)) {
            *pedal = frame[3];
        }

        match worker {
            Worker::OnTime => {
                while let Some(buffer) = self.ring.pop_front() {
                    for slot in &self.streams.input {
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
        played
    }

    fn run(&mut self, cycles: usize) {
        for _ in 0..cycles {
            self.cycle(Worker::OnTime);
        }
    }
}

// ── What a notification must leave alone ────────────────────────────────────

/// The chain as the stream layer and the control plane see it.
struct Snapshot<I, G, S, O> {
    identity: I,
    graph: BTreeMap<usize, usize>,
    slots: BTreeMap<usize, usize>,
    generation: G,
    stream_generation: S,
    owned: O,
    /// (rebuilds, activations) queued.
    pending: (usize, usize),
}

fn snapshot(
    controller: &mut ProjectRuntimeController,
) -> Snapshot<impl PartialEq, impl PartialEq, impl PartialEq, impl PartialEq> {
    let id = chain_id();
    Snapshot {
        identity: controller.runtime_identity(&id),
        graph: graph_ptrs(controller),
        slots: slot_ptrs(controller),
        generation: controller.active_chains.get(&id).map(|a| a.generation),
        stream_generation: controller.stream_generation,
        owned: controller
            .streams
            .owned(&id)
            .map(|o| (o.generation, o.input_streams, o.output_streams)),
        pending: (
            controller.pending_rebuilds.len(),
            controller.pending_activations.len(),
        ),
    }
}

/// The streams moved: new generation, or another count of streams.
fn streams_moved<I, G: PartialEq, S: PartialEq, O: PartialEq>(
    before: &Snapshot<I, G, S, O>,
    now: &Snapshot<I, G, S, O>,
) -> Option<String> {
    if now.generation != before.generation
        || now.stream_generation != before.stream_generation
        || now.owned != before.owned
    {
        Some(
            "the chain's streams were rebuilt (the stream generation or the number of \
             streams changed): the device was closed and reopened"
                .to_string(),
        )
    } else {
        None
    }
}

/// What changed between two snapshots of a chain nobody edited.
fn rebuilt<I: PartialEq, G: PartialEq, S: PartialEq, O: PartialEq>(
    before: &Snapshot<I, G, S, O>,
    now: &Snapshot<I, G, S, O>,
) -> Vec<String> {
    let mut failures = Vec::new();
    if now.identity != before.identity {
        failures.push("the chain's runtime identity changed: it was rebuilt".to_string());
    }
    if now.graph != before.graph {
        failures.push(format!(
            "the graph holds other runtimes than before (group: runtime {:?} -> {:?})",
            before.graph, now.graph
        ));
    }
    if now.slots != before.slots {
        failures.push(
            "the live slots publish other runtimes than before: the streams now play a \
             rebuilt chain"
                .to_string(),
        );
    }
    failures.extend(streams_moved(before, now));
    if now.pending != (0, 0) {
        failures.push(format!(
            "a rebuild of the chain is on its way ((rebuilds, activations) queued: {:?})",
            now.pending
        ));
    }
    failures
}

/// (group, route) -> (callbacks, underruns, latency trims), as
/// `openrig://routes` reads them.
type Counters = BTreeMap<(usize, usize), (u64, u64, u64)>;

fn counters(controller: &ProjectRuntimeController) -> Counters {
    let mut map = Counters::new();
    for (group, rows) in controller.chain_output_route_stats(&chain_id()) {
        for row in rows {
            map.insert(
                (group, row.route),
                (
                    row.callbacks as u64,
                    row.underruns as u64,
                    row.latency_trims as u64,
                ),
            );
        }
    }
    map
}

/// A counter that goes back, or a route that comes or goes, is a rebuilt
/// route: the owner saw callbacks reset and the send `[3]` come and go.
fn went_back(before: &Counters, now: &Counters) -> Vec<String> {
    let mut failures = Vec::new();
    for (key, was) in before {
        match now.get(key) {
            None => failures.push(format!(
                "group {} route {} disappeared from the route stats",
                key.0, key.1
            )),
            Some(is) if is.0 < was.0 || is.1 < was.1 || is.2 < was.2 => failures.push(format!(
                "group {} route {} (callbacks, underruns, trims) went {was:?} -> {is:?}: the \
                 route was rebuilt",
                key.0, key.1
            )),
            Some(_) => {}
        }
    }
    for key in now.keys().filter(|key| !before.contains_key(key)) {
        failures.push(format!(
            "group {} route {} appeared in the route stats",
            key.0, key.1
        ));
    }
    failures
}

/// Per output route, (peak, energy) of every buffer its stream handed the
/// device. A gap, a fade, a doubled copy or a shift of one frame all change it.
type Heard = BTreeMap<usize, Vec<(f32, f32)>>;

fn record(heard: &mut Heard, played: Vec<(usize, Vec<f32>)>) {
    for (route, out) in played {
        let peak = out.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let energy = out.iter().map(|s| s * s).sum::<f32>();
        heard.entry(route).or_default().push((peak, energy));
    }
}

fn heard_apart(quiet: &Heard, notified: &Heard) -> Vec<String> {
    let mut failures = Vec::new();
    let routes: BTreeSet<usize> = quiet.keys().chain(notified.keys()).copied().collect();
    for route in routes {
        let (Some(q), Some(n)) = (quiet.get(&route), notified.get(&route)) else {
            failures.push(format!("route {route} plays in only one of the two rigs"));
            continue;
        };
        let differs = |i: usize| match (q.get(i), n.get(i)) {
            (Some(a), Some(b)) => {
                (a.0 - b.0).abs() > EPS || (a.1 - b.1).abs() > EPS * a.1.abs().max(1.0)
            }
            _ => true,
        };
        if let Some(buffer) = (0..q.len().max(n.len())).find(|&i| differs(i)) {
            failures.push(format!(
                "route {route}: from buffer {buffer} on it sounds different, (peak, energy) \
                 {:?} with no notification vs {:?} with it",
                q.get(buffer),
                n.get(buffer)
            ));
        }
    }
    failures
}

fn counters_apart(quiet: &Counters, notified: &Counters) -> Vec<String> {
    let keys: BTreeSet<&(usize, usize)> = quiet.keys().chain(notified.keys()).collect();
    keys.into_iter()
        .filter(|key| quiet.get(*key) != notified.get(*key))
        .map(|key| {
            format!(
                "group {} route {} (callbacks, underruns, trims): {:?} with no notification, \
                 {:?} with it",
                key.0,
                key.1,
                quiet.get(key),
                notified.get(key)
            )
        })
        .collect()
}

// ── The notification, through the app's doors ───────────────────────────────

/// What the app runs when CoreAudio reports that the device list, or a
/// property of a device, changed: the cached enumeration is dropped (#693,
/// #829) and the E/S registry re-derived from the list is pushed into the
/// runtime (`SetIoBindings`, #716/#127).
fn device_notification(controller: &mut ProjectRuntimeController, bindings: &[IoBinding]) {
    crate::invalidate_device_cache();
    controller.set_io_bindings(bindings.to_vec());
}

/// The two checks `sync_live_chain_runtime` runs before it touches a live
/// chain. "Changed" from either is a full stream rebuild: `chain_io_changed`
/// drops the streams (`remove_chain`), `schedule_chain_activation` builds
/// brand-new ones (#881).
fn decision_doors(
    controller: &mut ProjectRuntimeController,
    project: &Project,
    chain: &Chain,
) -> Vec<String> {
    let mut failures = Vec::new();
    if controller
        .chain_io_changed(project, chain)
        .expect("the re-bind check must answer")
    {
        failures.push(
            "`chain_io_changed` reads the chain as re-bound: the app drops its streams and \
             opens new ones"
                .to_string(),
        );
    }
    if controller
        .schedule_chain_activation(project, chain)
        .expect("the activation door must answer")
    {
        failures.push(
            "`schedule_chain_activation` reads the chain as a new topology: it builds \
             brand-new streams"
                .to_string(),
        );
    }
    failures
}

/// Two copies of the same live rig fed the same guitars cycle for cycle. Both
/// run the install tick; only `notified` gets the notifications. Whatever the
/// notification does that a rig left alone does not is heard as the difference.
struct Pair {
    quiet: ProjectRuntimeController,
    notified: ProjectRuntimeController,
    quiet_hal: Hal,
    notified_hal: Hal,
    heard_quiet: Heard,
    heard_notified: Heard,
    /// Rebuilds the install tick swapped into the notified rig.
    installed: usize,
}

impl Pair {
    fn new(scene: Scene, bindings: &[IoBinding]) -> Self {
        let chain = chain_for(scene);
        let quiet = live_rig(scene, bindings.to_vec());
        let notified = live_rig(scene, bindings.to_vec());
        let quiet_hal = Hal::new(open_streams(&quiet, &chain));
        let notified_hal = Hal::new(open_streams(&notified, &chain));
        Self {
            quiet,
            notified,
            quiet_hal,
            notified_hal,
            heard_quiet: Heard::new(),
            heard_notified: Heard::new(),
            installed: 0,
        }
    }

    fn run(&mut self, cycles: usize) {
        for step in 1..=cycles {
            record(&mut self.heard_quiet, self.quiet_hal.cycle(Worker::OnTime));
            record(
                &mut self.heard_notified,
                self.notified_hal.cycle(Worker::OnTime),
            );
            if step % INSTALL_TICK == 0 {
                self.quiet.poll_pending_rebuilds();
                self.installed += self.notified.poll_pending_rebuilds();
            }
        }
    }

    /// Where the notified rig was heard or counted differently.
    fn apart(&self) -> Vec<String> {
        let mut failures = heard_apart(&self.heard_quiet, &self.heard_notified);
        failures.extend(counters_apart(
            &counters(&self.quiet),
            &counters(&self.notified),
        ));
        if self.installed != 0 {
            failures.push(format!(
                "the install tick swapped {} rebuild(s) into the notified chain",
                self.installed
            ));
        }
        failures
    }
}

/// The app's path for a live edit (`sync_live_chain_runtime`) once the checks
/// said "same streams": the off-thread rebuild door, then the install tick
/// swaps it in while the audio keeps running. Returns whether the door took it.
fn switch_live(
    controller: &mut ProjectRuntimeController,
    hal: &mut Hal,
    project: &Project,
    next: &Chain,
) -> bool {
    let took = controller
        .request_offthread_rebuild_if_live(project, next)
        .expect("the live edit door must answer");
    if !took {
        return false;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut applied = 0;
    while applied == 0 || !controller.pending_rebuilds.is_empty() {
        hal.cycle(Worker::OnTime);
        applied += controller.poll_pending_rebuilds();
        assert!(Instant::now() < deadline, "the rebuild never landed");
        std::thread::sleep(Duration::from_micros(300));
    }
    true
}

fn report(failures: &[String]) -> String {
    let shown: Vec<&String> = failures.iter().take(40).collect();
    let more = failures.len().saturating_sub(shown.len());
    let mut text = shown
        .iter()
        .map(|f| f.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if more > 0 {
        text.push_str(&format!("\n... and {more} more"));
    }
    text
}

// ── Critic 4: an unchanged device notification never rebuilds the chain ────

/// The 40-minute watch, compressed into 30 s of playing per grouping. Both
/// guitars play while every tick the app runs keeps turning (the 5 ms install
/// tick, the 200 ms error poll, the 0.5 s `openrig://routes` read with the
/// meter tick, the 2 s health tick) and CoreAudio keeps talking: the HD 8
/// re-enumerated about every 3.5 s, another device taking and giving back the
/// system default about every 4.4 s, another app reopening the HD 8 at the same
/// settings about every 5.8 s (its IO stops ~50 ms while a burst of property
/// notifications lands), and an overload about every 2.9 s (the worker falls 7
/// buffers behind, the #670 recovery drops its backlog, the callbacks record an
/// over-budget load). None of it is an edit. After all of it the chain must be
/// the one that started, no route counter may ever go back, and the health
/// tick must never see a reason to reconnect.
#[test]
fn an_unchanged_device_notification_never_rebuilds_the_chain() {
    /// 30 s of 64-frame cycles at 44.1 kHz.
    const CYCLES: usize = 20_672;
    const ERROR_TICK: usize = 138;
    const DEVICE_LIST: usize = 2_411;
    const OTHER_DEVICE: usize = 3_001;
    const REOPEN: usize = 4_003;
    const OVERLOAD: usize = 1_999;
    /// ~50 ms with no callback while the other app reconfigures the HD 8.
    const REOPEN_GAP: usize = 35;
    /// Property notifications CoreAudio delivers for one reopen.
    const BURST: usize = 6;

    let id = chain_id();
    let mut failures = Vec::new();
    for scene in [ANAL_DIG, SYN2] {
        let state = state_of(scene);
        let chain = chain_for(scene);
        let mut default_device = BUILT_IN;
        let mut controller = live_rig(scene, machine_registry(Some(default_device)));
        let mut hal = Hal::new(open_streams(&controller, &chain));
        hal.run(TWO_SECONDS);
        assert!(
            controller.is_healthy(),
            "setup ({state}): the rig must read healthy before any notification"
        );

        let before = snapshot(&mut controller);
        let mut last = counters(&controller);
        let mut xruns = controller.chain_xrun_count(&id);
        let mut installed = 0usize;
        let mut behind = 0usize;
        let mut silent = 0usize;
        let mut unhealthy_at: Option<usize> = None;
        for cycle in 1..=CYCLES {
            if cycle % OVERLOAD == 0 {
                behind = 8;
            }
            if cycle % REOPEN == 0 {
                silent = REOPEN_GAP;
                for _ in 0..BURST {
                    device_notification(&mut controller, &machine_registry(Some(default_device)));
                }
            }
            if cycle % DEVICE_LIST == 0 {
                device_notification(&mut controller, &machine_registry(Some(default_device)));
            }
            if cycle % OTHER_DEVICE == 0 {
                default_device = if default_device == BUILT_IN {
                    AGGREGATE
                } else {
                    BUILT_IN
                };
                device_notification(&mut controller, &machine_registry(Some(default_device)));
            }

            if silent > 0 {
                silent -= 1;
            } else {
                let worker = match behind {
                    0 => Worker::OnTime,
                    1 => Worker::DropsBacklog,
                    _ => Worker::Late,
                };
                behind = behind.saturating_sub(1);
                hal.cycle(worker);
                if worker == Worker::DropsBacklog {
                    for (_, runtime) in live(&controller) {
                        for _ in 0..8 {
                            runtime.record_callback_load(4_000_000, 1_451_000);
                        }
                    }
                }
            }

            if cycle % INSTALL_TICK == 0 {
                installed += controller.poll_pending_rebuilds();
            }
            if cycle % ERROR_TICK == 0 {
                let now = controller.chain_xrun_count(&id);
                if now < xruns {
                    failures.push(format!(
                        "{state}, cycle {cycle}: the overload count went {xruns:?} -> {now:?}: \
                         the runtime that counted them was replaced"
                    ));
                }
                xruns = now;
            }
            if cycle % TWO_SECONDS == 0 && unhealthy_at.is_none() && !controller.is_healthy() {
                unhealthy_at = Some(cycle);
            }
            if cycle % HALF_SECOND == 0 {
                let now = counters(&controller);
                failures.extend(
                    went_back(&last, &now)
                        .into_iter()
                        .map(|f| format!("{state}, cycle {cycle}: {f}")),
                );
                last = now;
                let _ = controller.runtime_identity(&id);
                controller.sync_looper_streams(&chain);
            }
        }
        assert!(
            !last.is_empty() && last.values().all(|(callbacks, _, _)| *callbacks > 0),
            "setup ({state}): every route of the chain must have played: {last:?}"
        );

        if let Some(cycle) = unhealthy_at {
            failures.push(format!(
                "{state}, cycle {cycle}: `is_healthy` read false with the HD 8 still there: \
                 the 2 s tick reconnects, tearing down and reopening every stream"
            ));
        }
        if installed != 0 {
            failures.push(format!(
                "{state}: the install tick swapped {installed} rebuild(s) into a chain nobody \
                 edited"
            ));
        }
        failures.extend(
            rebuilt(&before, &snapshot(&mut controller))
                .into_iter()
                .map(|f| format!("{state}: {f}")),
        );
    }
    assert!(
        failures.is_empty(),
        "a device notification that changed nothing rebuilt the live chain (measured: 3 \
         rebuilds in 40 min with nobody touching the rig):\n{}",
        report(&failures)
    );
}

/// The device list changed and the refresh found the HD 8 exactly as it was
/// (another device's hot-plug, a refresh from MCP, coreaudiod re-announcing
/// its devices). The app drops its device cache, pushes the unchanged E/S
/// registry into the runtime, and the drain asks the live-edit checks whether
/// the chain moved. Both must answer "no", nothing may be rebuilt, and every
/// stream must sound exactly like the same rig that got no notification, buffer
/// for buffer: no gap, no fade, no extra latency, no underrun or trim.
#[test]
fn a_device_list_refresh_that_finds_the_same_hd8_changes_nothing() {
    let mut failures = Vec::new();
    for scene in [ANAL_DIG, SYN2] {
        let state = state_of(scene);
        let chain = chain_for(scene);
        let project = project_for(&chain);
        let bindings = machine_registry(Some(BUILT_IN));
        let mut pair = Pair::new(scene, &bindings);
        let setup = decision_doors(&mut pair.notified, &project, &chain);
        assert!(
            setup.is_empty(),
            "setup ({state}): before any notification the live-edit checks must read the \
             unchanged chain as unchanged (the rig stores the signatures an activation \
             stores): {setup:?}"
        );
        pair.run(TWO_SECONDS);
        let before = snapshot(&mut pair.notified);

        for refresh in 1..=K {
            device_notification(&mut pair.notified, &bindings);
            failures.extend(
                decision_doors(&mut pair.notified, &project, &chain)
                    .into_iter()
                    .map(|f| format!("{state}, refresh {refresh}: {f}")),
            );
            pair.run(HALF_SECOND);
        }
        failures.extend(
            rebuilt(&before, &snapshot(&mut pair.notified))
                .into_iter()
                .map(|f| format!("{state}: {f}")),
        );
        failures.extend(pair.apart().into_iter().map(|f| format!("{state}: {f}")));
    }
    assert!(
        failures.is_empty(),
        "a device-list refresh that found the same HD 8 touched the live chain:\n{}",
        report(&failures)
    );
}

/// Another device comes and goes while the owner plays: another app creates
/// an aggregate device and it takes the system default, it goes away, a USB
/// headset is plugged in and becomes the default, it is unplugged and the
/// `default` E/S has no device, then the built-in output is back. Each time the
/// refresh moves the machine's `default` E/S (#716) and pushes the registry.
/// `rig:input-7` selects none of it: N streams are N isolated pipelines, so a
/// change to another device or another E/S never touches this chain's
/// runtimes, streams or sound, and the live-edit checks still read it as
/// unchanged.
#[test]
fn another_device_coming_or_going_never_touches_this_chain() {
    let moves: [(&str, Option<&str>); 5] = [
        (
            "an aggregate device takes the system default",
            Some(AGGREGATE),
        ),
        ("the aggregate device goes away", Some(BUILT_IN)),
        ("a USB headset is plugged in", Some(HEADSET)),
        ("the headset is unplugged", None),
        ("the built-in output is the default again", Some(BUILT_IN)),
    ];
    let mut failures = Vec::new();
    for scene in [ANAL_DIG, SYN2] {
        let state = state_of(scene);
        let chain = chain_for(scene);
        let project = project_for(&chain);
        let mut pair = Pair::new(scene, &machine_registry(Some(BUILT_IN)));
        let setup = decision_doors(&mut pair.notified, &project, &chain);
        assert!(
            setup.is_empty(),
            "setup ({state}): before any notification the live-edit checks must read the \
             unchanged chain as unchanged: {setup:?}"
        );
        pair.run(TWO_SECONDS);
        let before = snapshot(&mut pair.notified);

        for (what, default_device) in moves {
            device_notification(&mut pair.notified, &machine_registry(default_device));
            failures.extend(
                decision_doors(&mut pair.notified, &project, &chain)
                    .into_iter()
                    .map(|f| format!("{state}, {what}: {f}")),
            );
            pair.run(HALF_SECOND);
        }
        failures.extend(
            rebuilt(&before, &snapshot(&mut pair.notified))
                .into_iter()
                .map(|f| format!("{state}: {f}")),
        );
        failures.extend(pair.apart().into_iter().map(|f| format!("{state}: {f}")));
    }
    assert!(
        failures.is_empty(),
        "a device this chain does not use came or went, and the chain was touched:\n{}",
        report(&failures)
    );
}

/// Another app opens the HD 8 at the settings it already runs (44.1 kHz, 64
/// frames). CoreAudio stops the device's IO for a moment, delivers a burst of
/// notifications (the device list, the nominal rate, the buffer size, all at
/// the values they had), then the IO resumes in order: the input callback
/// first, then the outputs. No callback ran while the IO was stopped, so there
/// is nothing to lose. The HD 8 never went away, so the health tick must not
/// see a reason to reconnect, nothing may be rebuilt, and every stream must
/// sound exactly like a rig nobody reopened.
#[test]
fn another_app_reopening_the_hd8_at_the_same_settings_is_not_heard() {
    /// Notifications CoreAudio delivers for one reopen.
    const BURST: usize = 6;
    let mut failures = Vec::new();
    for scene in [ANAL_DIG, SYN2] {
        let state = state_of(scene);
        let bindings = machine_registry(Some(BUILT_IN));
        let mut pair = Pair::new(scene, &bindings);
        pair.run(TWO_SECONDS);
        assert!(
            pair.notified.is_healthy(),
            "setup ({state}): the rig must read healthy before any notification"
        );
        let before = snapshot(&mut pair.notified);

        for reopen in 1..=K {
            for _ in 0..BURST {
                device_notification(&mut pair.notified, &bindings);
                pair.installed += pair.notified.poll_pending_rebuilds();
            }
            if !pair.notified.is_healthy() {
                failures.push(format!(
                    "{state}, reopen {reopen}: `is_healthy` read false while another app \
                     reopened the HD 8: the 2 s tick reconnects, tearing down and reopening \
                     every stream"
                ));
            }
            pair.run(HALF_SECOND);
        }
        failures.extend(
            rebuilt(&before, &snapshot(&mut pair.notified))
                .into_iter()
                .map(|f| format!("{state}: {f}")),
        );
        failures.extend(pair.apart().into_iter().map(|f| format!("{state}: {f}")));
    }
    assert!(
        failures.is_empty(),
        "another app reopening the HD 8 at the same settings was heard or rebuilt the \
         chain:\n{}",
        report(&failures)
    );
}

/// An overload and its recovery: the worker falls 7 buffers behind, the #670
/// saturation recovery drops its backlog, and the callbacks record an
/// over-budget load (the chain's overload LED). That costs the dropped
/// buffers, and nothing else: an overload is not a dead device, so the health
/// tick must never reconnect, the runtime that counts the overloads must stay
/// (the count never goes back), and nothing may be rebuilt or reset.
#[test]
fn an_overload_and_its_recovery_never_reconnect_or_rebuild_the_chain() {
    const OVERLOADS: usize = 8;
    /// ~1.4 s between overloads.
    const EVERY: usize = 997;
    const ERROR_TICK: usize = 138;
    let id = chain_id();
    let mut failures = Vec::new();
    for scene in [ANAL_DIG, SYN2] {
        let state = state_of(scene);
        let chain = chain_for(scene);
        let mut controller = live_rig(scene, machine_registry(Some(BUILT_IN)));
        let mut hal = Hal::new(open_streams(&controller, &chain));
        hal.run(TWO_SECONDS);
        assert!(
            controller.is_healthy(),
            "setup ({state}): the rig must read healthy before any overload"
        );
        let before = snapshot(&mut controller);
        let mut last = counters(&controller);
        let first_xruns = controller.chain_xrun_count(&id);
        let mut xruns = controller.chain_xrun_count(&id);
        let mut installed = 0usize;

        for overload in 1..=OVERLOADS {
            for step in 0..EVERY {
                let worker = match step {
                    0..=6 => Worker::Late,
                    7 => Worker::DropsBacklog,
                    _ => Worker::OnTime,
                };
                hal.cycle(worker);
                if worker == Worker::DropsBacklog {
                    for (_, runtime) in live(&controller) {
                        for _ in 0..8 {
                            runtime.record_callback_load(4_000_000, 1_451_000);
                        }
                    }
                }
                if step % INSTALL_TICK == 0 {
                    installed += controller.poll_pending_rebuilds();
                }
                if step % ERROR_TICK == 0 {
                    let now = controller.chain_xrun_count(&id);
                    if now < xruns {
                        failures.push(format!(
                            "{state}, overload {overload}: the overload count went {xruns:?} \
                             -> {now:?}: the runtime that counted them was replaced"
                        ));
                    }
                    xruns = now;
                }
            }
            if !controller.is_healthy() {
                failures.push(format!(
                    "{state}, overload {overload}: `is_healthy` read false after an overload: \
                     the 2 s tick reconnects, tearing down and reopening every stream"
                ));
            }
            let now = counters(&controller);
            failures.extend(
                went_back(&last, &now)
                    .into_iter()
                    .map(|f| format!("{state}, overload {overload}: {f}")),
            );
            last = now;
        }
        assert!(
            controller.chain_xrun_count(&id) > first_xruns,
            "setup ({state}): the over-budget callbacks must have lit the overload count"
        );
        if installed != 0 {
            failures.push(format!(
                "{state}: the install tick swapped {installed} rebuild(s) in after overloads"
            ));
        }
        failures.extend(
            rebuilt(&before, &snapshot(&mut controller))
                .into_iter()
                .map(|f| format!("{state}: {f}")),
        );
    }
    assert!(
        failures.is_empty(),
        "an overload and its recovery rebuilt or reconnected the chain:\n{}",
        report(&failures)
    );
}

/// A device notification must not leave the chain in a state where the next
/// real edit costs more. After the refresh (the same HD 8, the `default` E/S
/// moving between the built-in output and a headset), a footswitch scene
/// switch that keeps the loop where it was is still what it is on a rig that
/// got no notification: the live-edit checks read "same streams", the #967
/// off-thread door takes it, and it lands on the streams the chain opened.
/// The stream generation and counts never move, the live slots stay in
/// lock-step with the graph, and every captured stream pops exactly the
/// runtime that writes its route.
#[test]
fn after_a_device_notification_a_scene_switch_keeps_the_streams() {
    let mut failures = Vec::new();
    for scenes in SAME_LOOP_SCENES {
        let state = state_of(scenes[0]);
        let mut controller = live_rig(scenes[0], machine_registry(Some(BUILT_IN)));
        let mut hal = Hal::new(open_streams(&controller, &chain_for(scenes[0])));
        hal.run(HALF_SECOND);
        let first = chain_for(scenes[1]);
        let setup = decision_doors(&mut controller, &project_for(&first), &first);
        assert!(
            setup.is_empty(),
            "setup ({state}): with no notification a switch to {:?} must read as the same \
             streams (#967): {setup:?}",
            scenes[1]
        );
        let before = snapshot(&mut controller);

        for k in 1..=K {
            let default_device = if k % 2 == 1 { HEADSET } else { BUILT_IN };
            device_notification(&mut controller, &machine_registry(Some(BUILT_IN)));
            device_notification(&mut controller, &machine_registry(Some(default_device)));
            hal.run(HALF_SECOND);

            let next_scene = scenes[k % 2];
            let when = format!("{state}, switch {k} -> {next_scene:?} after a notification");
            let next = chain_for(next_scene);
            let project = project_for(&next);
            let doors = decision_doors(&mut controller, &project, &next);
            if !doors.is_empty() {
                failures.extend(doors.into_iter().map(|f| format!("{when}: {f}")));
                continue;
            }
            if !switch_live(&mut controller, &mut hal, &project, &next) {
                failures.push(format!(
                    "{when}: the #967 off-thread door refused a switch that keeps the loop \
                     where it was"
                ));
                continue;
            }
            hal.run(SETTLE);
            if let Some(f) = streams_moved(&before, &snapshot(&mut controller)) {
                failures.push(format!("{when}: {f}"));
            }
            failures.extend(
                stream_layer_mismatches(&controller, &hal.streams)
                    .into_iter()
                    .map(|f| format!("{when}: {f}")),
            );
        }
    }
    assert!(
        failures.is_empty(),
        "after a device notification a scene switch no longer kept the chain's streams:\n{}",
        report(&failures)
    );
}
