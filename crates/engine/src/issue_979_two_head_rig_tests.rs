//! #979 — the owner's `rig:input-7` on the Quantum HD 8, rebuilt exactly:
//! 44.1 kHz, 64-frame buffer, two mono guitar heads (`guitarra-1` on In 1,
//! `guitarra-2` on In 2), each E/S going to Main `[0,1]` and Out `[10,11]`,
//! plus the `syn2-main` insert (send on USB playback 4 = channel `[3]`, which
//! the HD 8 mixer routes to Out 8; stereo return on In 3/4 = `[2,3]`).
//!
//! Symptom: after minutes of playing, `latency_trims` rise and every route's
//! underruns climb in whole 64-frame buffers until the chain is rebuilt, while
//! both DSP workers are ~28% busy. Heard as a stuttering "loop".
//!
//! The HAL cycle is simulated the way #965 measured it on this interface: every
//! CoreAudio unit of the device runs on one IO thread, the input callback
//! first and every output callback right after, and the chain DSP runs on the
//! #670 worker AFTER the output callbacks of that cycle. A late worker is a
//! worker that has not pushed before the next cycle's output callbacks.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::{process_input_f32, process_output_f32};
use crate::runtime_graph::build_per_input_runtimes;
use crate::runtime_state::ChainRuntimeState;

const HD8: &str = "coreaudio:TUSBAudio:Fender:Quantum HD 8";
/// The HD 8 exposes 30 channels each way to CoreAudio.
const HD8_CHANNELS: usize = 30;
const RATE: f32 = 44_100.0;
/// The owner's device buffer.
const FRAMES: usize = 64;
/// What `infra-cpal`'s `elastic_targets` gives every route of this chain on
/// macOS: each route's single producer is the HD 8 itself, so ×1 the buffer
/// (pinned in infra-cpal `elastic_tests::a_regular_output_on_the_input_device_rests_one_buffer_on_macos`).
const TARGET: usize = 64;
/// Output routes of the chain: Main and Out 2 of each head, then the send.
const ROUTES: usize = 5;

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
pub(crate) fn rig_registry() -> Vec<IoBinding> {
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

fn gain(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

/// `rig:input-7`: the insert first, then the preamp and cab (volume blocks
/// stand in for NAM and IR — the timing under test is the route's, not the
/// DSP's).
pub(crate) fn rig_chain(insert_enabled: bool) -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into(), "guitarra-2".into()],
        blocks: vec![
            AudioBlock {
                id: BlockId("syn2-main".into()),
                enabled: insert_enabled,
                kind: AudioBlockKind::Insert(InsertBlock {
                    model: "standard".into(),
                    io: "syn2-main".into(),
                }),
            },
            gain("preamp"),
            gain("cab"),
        ],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn runtimes(insert_enabled: bool) -> Vec<Arc<ChainRuntimeState>> {
    build_per_input_runtimes(
        &rig_chain(insert_enabled),
        RATE,
        &HashMap::new(),
        &[TARGET; ROUTES],
        &rig_registry(),
    )
    .expect("the owner's chain must build")
    .into_iter()
    .map(|(_, state)| Arc::new(state))
    .collect()
}

/// Channels of every route a runtime writes, by route index.
fn written_routes(runtime: &ChainRuntimeState) -> Vec<(usize, Vec<usize>)> {
    runtime
        .output_routes
        .load()
        .iter()
        .enumerate()
        .filter_map(|(idx, r)| r.as_ref().map(|r| (idx, r.output_channels.clone())))
        .collect()
}

/// (input channels, routes written) of every pipeline in a runtime.
fn pipelines(runtime: &ChainRuntimeState) -> Vec<(Vec<usize>, Vec<usize>)> {
    let processing = runtime.processing.lock().unwrap();
    processing
        .input_states
        .iter()
        .map(|s| (s.input_channels.clone(), s.output_route_indices.clone()))
        .collect()
}

/// Route `[3]` is the insert send: the binding's send endpoint is channel 3
/// (USB playback 4, which the HD 8 mixer sends to Out 8), appended after the
/// four head routes. It is not a stray route.
#[test]
fn route_3_is_the_insert_send() {
    for enabled in [true, false] {
        let all: Vec<(usize, Vec<usize>)> = runtimes(enabled)
            .iter()
            .flat_map(|rt| written_routes(rt))
            .collect();
        let send: Vec<&(usize, Vec<usize>)> = all.iter().filter(|(_, ch)| ch == &[3]).collect();
        if enabled {
            assert_eq!(
                send,
                vec![&(4, vec![3])],
                "insert on: the send is route 4 on [3]"
            );
        } else {
            assert!(
                send.is_empty(),
                "insert off: nothing writes the send, got {all:?}"
            );
        }
    }
}

/// Hypothesis 1: the doubled `input entry processing layout … channels=[0]`
/// log is two runtimes processing one head into one route. It is not: each
/// head is ONE runtime with ONE pipeline fanning out to its outputs (#1074),
/// and no route has two writers or two owning runtimes.
// linux+JACK keeps the pre-#967 grouping (one runtime per chain).
#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn insert_off_every_route_has_exactly_one_writing_pipeline() {
    let rts = runtimes(false);
    let mut writers: HashMap<usize, usize> = HashMap::new();
    for rt in &rts {
        for (_, routes) in pipelines(rt) {
            for route in routes {
                *writers.entry(route).or_default() += 1;
            }
        }
    }
    assert!(
        writers.values().all(|n| *n == 1) && writers.len() == 4,
        "a route has two writers: {writers:?}; pipelines {:?}",
        rts.iter().map(|rt| pipelines(rt)).collect::<Vec<_>>()
    );
    let pipelines_per_guitar: Vec<usize> = rts.iter().map(|rt| pipelines(rt).len()).collect();
    assert_eq!(
        pipelines_per_guitar,
        vec![1, 1],
        "#1074: one pipeline per guitar, fanning out to both outputs"
    );
}

/// Insert on: the chain is one runtime; each head's pipeline writes the send
/// (both guitars feed the pedals, the #967 design), and ONE return pipeline
/// feeds the tail — once per physical output (#979): Main `[0,1]` through
/// route 0 and `[10,11]` through route 1. Routes 2 and 3 are `guitarra-2`'s
/// own routes on those same outputs; nothing writes them with the loop on.
#[test]
fn insert_on_heads_feed_the_send_and_the_return_feeds_the_tail() {
    let rts = runtimes(true);
    assert_eq!(rts.len(), 1, "an insert chain is one runtime");
    assert_eq!(
        pipelines(&rts[0]),
        vec![
            (vec![0], vec![4]),
            (vec![1], vec![4]),
            (vec![2, 3], vec![0, 1]),
        ]
    );
}

/// OPEN (#979, decision for the owner): with the insert on, the one return
/// pipeline writes BOTH heads' routes, and those are the same physical
/// channels — routes 0 and 2 are both Main `[0,1]`, routes 1 and 3 both
/// `[10,11]`. The device sums the two copies: +6 dB, and when the two routes
/// sit a buffer apart (the first capture: route 0 at fill 64, route 2 at 128)
/// the same signal plays twice, 1.45 ms apart — a comb, "several streams
/// stacked". Which route should carry the return is a routing decision, so
/// this stays red until it is made.
#[test]
fn insert_on_the_return_reaches_each_physical_output_once() {
    let rts = runtimes(true);
    let routes = written_routes(&rts[0]);
    let mut copies: HashMap<Vec<usize>, usize> = HashMap::new();
    for (_, routes_written) in pipelines(&rts[0])
        .into_iter()
        .filter(|(input, _)| input == &vec![2, 3])
    {
        for route in routes_written {
            let channels = routes.iter().find(|(r, _)| *r == route).unwrap().1.clone();
            *copies.entry(channels).or_default() += 1;
        }
    }
    assert!(
        copies.values().all(|n| *n == 1),
        "the insert return is played {copies:?} times per physical output"
    );
}

/// With the insert off the chain is two isolated runtimes (one per guitar),
/// each writing only its own E/S's Main and Out 2 — the live capture's two
/// groups of `[0,1]` + `[10,11]`, and the two dsp-workers.
// linux+JACK keeps the pre-#967 grouping (one runtime per chain).
#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn insert_off_is_one_runtime_per_guitar_on_its_own_outputs() {
    let rts = runtimes(false);
    assert_eq!(rts.len(), 2);
    assert_eq!(
        written_routes(&rts[0]),
        vec![(0, vec![0, 1]), (1, vec![10, 11])]
    );
    assert_eq!(
        written_routes(&rts[1]),
        vec![(2, vec![0, 1]), (3, vec![10, 11])]
    );
    for (rt, ch) in rts.iter().zip([0usize, 1]) {
        for (input, _) in pipelines(rt) {
            assert_eq!(input, vec![ch], "a runtime only reads its own guitar");
        }
    }
}

/// One simulated HAL cycle of the whole chain. `late` holds back the worker:
/// its push for this cycle lands after the NEXT cycle's output callbacks.
struct Hal {
    runtimes: Vec<Arc<ChainRuntimeState>>,
    input: Vec<f32>,
    out: Vec<f32>,
    /// Input buffers the worker received but has not processed yet.
    backlog: usize,
}

impl Hal {
    fn new(insert_enabled: bool) -> Self {
        Self {
            runtimes: runtimes(insert_enabled),
            input: vec![0.0; FRAMES * HD8_CHANNELS],
            out: vec![0.0; FRAMES * HD8_CHANNELS],
            backlog: 0,
        }
    }

    fn cycle(&mut self, worker_late: bool) {
        for frame in self.input.chunks_mut(HD8_CHANNELS) {
            frame[0] = 0.1;
            frame[1] = 0.1;
        }
        // Input callback: the buffer goes to the worker's ring.
        self.backlog += 1;
        // Output callbacks of this cycle, every route of every runtime.
        for rt in &self.runtimes {
            for (route, _) in written_routes(rt) {
                process_output_f32(rt, route, &mut self.out, HD8_CHANNELS);
            }
        }
        // The worker runs after them, unless it is late this cycle.
        if !worker_late {
            for _ in 0..self.backlog {
                for rt in &self.runtimes {
                    process_input_f32(rt, 0, &self.input, HD8_CHANNELS);
                }
            }
            self.backlog = 0;
        }
    }

    /// (underrun frames, latency trims) summed over every route.
    fn damage(&self) -> Vec<(usize, u64, u64)> {
        self.runtimes
            .iter()
            .flat_map(|rt| {
                rt.output_routes
                    .load()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| {
                        r.as_ref()
                            .map(|r| (i, r.buffer.underrun_count(), r.buffer.latency_trims()))
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

/// Hypothesis 6/7: a late worker is a real, bounded event — one buffer of
/// silence on each route, then the #953 guard sheds the buffer it left behind.
/// It must never turn into damage that keeps coming while the worker is on
/// time. The worker is late once every 997 cycles (~1.4 s: a busy machine), so
/// over ~3 minutes one late push lands on every position of the guard's
/// 128-callback window (997 is coprime with 128). #991: one lap, not two —
/// pre-#979 code already fails inside it (20928 underrun frames and 274 trims
/// for 128 late buffers on route 0), and the second lap only doubled the run.
#[test]
fn a_late_worker_costs_one_buffer_and_never_starts_a_stutter() {
    const LATE_EVERY: usize = 997;
    const CYCLES: usize = LATE_EVERY * 128;
    for insert_enabled in [false, true] {
        let mut hal = Hal::new(insert_enabled);
        // Warm-up: the routes start empty, so the first callback underruns.
        for _ in 0..1_000 {
            hal.cycle(false);
        }
        let warm = hal.damage();
        let mut late_events = 0u64;
        for cycle in 1..=CYCLES {
            let late = cycle % LATE_EVERY == 0;
            late_events += u64::from(late);
            hal.cycle(late);
        }
        for ((route, underruns, trims), (_, u0, t0)) in hal.damage().into_iter().zip(warm) {
            let (underruns, trims) = (underruns - u0, trims - t0);
            assert!(
                underruns <= late_events * FRAMES as u64 && trims <= late_events,
                "insert {insert_enabled}, route {route}: {late_events} late worker buffers \
                 cost {underruns} underrun frames and {trims} trims — the route kept \
                 starving after the worker was back on time"
            );
        }
    }
}

#[path = "issue_979_insert_bridge_isolation_tests.rs"]
mod insert_bridge_isolation;

#[path = "issue_979_cushion_shedding_969_tests.rs"]
mod cushion_shedding_969;

#[path = "issue_979_sibling_route_divergence_tests.rs"]
mod sibling_route_divergence;

#[path = "issue_979_late_worker_recovery_tests.rs"]
mod late_worker_recovery;

#[path = "issue_979_inplace_rebuild_scene_switch_tests.rs"]
mod inplace_rebuild_scene_switch;

#[path = "issue_979_switch_timing_tests.rs"]
mod switch_timing;

#[path = "issue_979_issue_body_topology_tests.rs"]
mod issue_body_topology;

#[path = "issue_979_real_block_probe_tests.rs"]
mod real_block_probe;
