//! #965 — an insert loop must not cost more latency than its own cushion.
//!
//! The owner's rig: ANAL+DIG on the Quantum HD 8 at 64 frames — guitar in,
//! insert to the SYN-2 (send ADAT out 9, return ADAT 2/3), then a preamp and
//! a cab IR, out to Main and the FRFR. Measured live: the send route AND both
//! tail routes sat at 1024 queued frames each (23 ms per hop, the ring's whole
//! capacity), against 512 on the acoustic chain next to it. Two defects add
//! up to it, each pinned below with the production build path.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::{process_input_f32, process_output_f32};
use crate::elastic_drift_guard::SLACK_FRAMES;
use crate::runtime_graph::{
    build_chain_runtime_state, build_chain_runtime_state_with_device_rates,
    build_per_input_runtimes,
};
use crate::runtime_state::ChainRuntimeState;

const QUANTUM: &str = "coreaudio:quantum";
const DEVICE_CHANNELS: usize = 26;
const FRAMES: usize = 64;
const GUITAR_IN: usize = 0;
const MAIN_OUT: [usize; 2] = [0, 1];
const SYN2_RETURN: [usize; 2] = [15, 16];
const SYN2_SEND: usize = 22;

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(QUANTUM.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "guitarra-1".into(),
            name: "Guitarra 1".into(),
            inputs: vec![endpoint("in", ChannelMode::Mono, &[GUITAR_IN])],
            outputs: vec![endpoint("main", ChannelMode::Stereo, &MAIN_OUT)],
        },
        IoBinding {
            id: "syn2-main".into(),
            name: "SYN2".into(),
            inputs: vec![endpoint("ret", ChannelMode::Stereo, &SYN2_RETURN)],
            outputs: vec![endpoint("snd", ChannelMode::Mono, &[SYN2_SEND])],
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

fn cab(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_CAB.to_string(),
            model: "ir_test_fake".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn insert(id: &str, io: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: io.into(),
        }),
    }
}

fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

/// Defect 1: the IR cold-start cushion (#592) is decided per CHAIN, so the
/// insert SEND — fed by the segment BEFORE the insert, which holds no IR at
/// all — got the same 512-frame cushion as the tail behind the cab. The send
/// was sized lean on purpose (`ELASTIC_MULTIPLIER_INSERT_SEND`); the chain-wide
/// floor silently undid that.
#[test]
fn the_insert_send_gets_no_ir_cushion_when_no_ir_feeds_it() {
    let owner = chain(vec![insert("insert:1", "syn2-main"), cab("cab")]);
    let tail_target = 128;
    let send_target = 64;
    let rt = build_chain_runtime_state(&owner, 44_100.0, &[tail_target, send_target], &registry())
        .expect("the chain must build");
    let routes = rt.output_routes.load();
    let send = routes[1].as_ref().expect("route 1 is the insert send");
    assert_eq!(
        send.buffer.target_level(),
        send_target,
        "the send route sits BEFORE the cab: nothing convolves into it, so it \
         must keep its own lean target, not the chain-wide IR cushion"
    );
    assert_eq!(
        send.buffer.len(),
        0,
        "and it must not be primed with silence either — every primed frame \
         is a frame of latency the loop pays for nothing"
    );
    let tail = routes[0].as_ref().expect("route 0 is the main tail");
    assert_eq!(
        tail.buffer.len(),
        tail_target,
        "the tail behind the cab is born at its own cushion"
    );
}

/// One route, no IR: the plain rig the second defect needs.
fn plain_runtime() -> Arc<ChainRuntimeState> {
    runtime_for(chain(vec![gain("amp")]))
}

fn runtime_for(owner: Chain) -> Arc<ChainRuntimeState> {
    let runtimes =
        build_per_input_runtimes(&owner, 44_100.0, &HashMap::new(), &[TARGET], &registry())
            .expect("the chain must build");
    assert_eq!(runtimes.len(), 1, "one binding, one input = one runtime");
    Arc::new(runtimes.into_iter().next().unwrap().1)
}

const TARGET: usize = 128;

struct Rig {
    runtime: Arc<ChainRuntimeState>,
    input: Vec<f32>,
    out: Vec<f32>,
}

impl Rig {
    fn new() -> Self {
        Self::with_runtime(plain_runtime())
    }

    fn with_runtime(runtime: Arc<ChainRuntimeState>) -> Self {
        Self {
            runtime,
            input: vec![0.0; FRAMES * DEVICE_CHANNELS],
            out: vec![0.0; FRAMES * DEVICE_CHANNELS],
        }
    }

    /// The input callback alone — the input stream running while the output
    /// stream has not started yet.
    fn input_period(&mut self, pulse: bool) {
        self.input.fill(0.0);
        if pulse {
            self.input[GUITAR_IN] = 0.5;
        }
        process_input_f32(&self.runtime, 0, &self.input, DEVICE_CHANNELS);
    }

    /// One device period with both streams running. Returns the first frame
    /// of this period carrying signal on the main route.
    fn period(&mut self, pulse: bool) -> Option<usize> {
        self.input_period(pulse);
        self.out.fill(0.0);
        process_output_f32(&self.runtime, 0, &mut self.out, DEVICE_CHANNELS);
        self.out
            .chunks_exact(DEVICE_CHANNELS)
            .position(|frame| frame[MAIN_OUT[0]].abs() > 1e-3)
    }

    /// Latency of the route, in frames, for a pulse sent now.
    fn pulse_latency(&mut self) -> usize {
        for p in 0..64 {
            if let Some(f) = self.period(p == 0) {
                return p * FRAMES + f;
            }
        }
        panic!("the pulse must come out of the route");
    }
}

/// Defect 2: the streams of a chain are started input first, output last, and
/// the output stream takes its time to deliver its first callback. Every
/// input period in between pushes a buffer nobody pops, so the ring is FULL by
/// the time the output runs — and the #953 guard, which learns the route's
/// resting level from the first clean window, ratified that full ring as
/// "normal". The route then carried the whole capacity as latency until the
/// chain was switched off and on.
#[test]
fn a_route_whose_input_ran_ahead_of_its_output_settles_back_to_its_cushion() {
    let mut rig = Rig::new();
    // The input stream delivers 16 periods before the output stream starts.
    for _ in 0..16 {
        rig.input_period(false);
    }
    // Then the rig plays for ~3 s.
    for _ in 0..2_000 {
        rig.period(false);
    }
    let latency = rig.pulse_latency();
    assert!(
        latency <= TARGET + SLACK_FRAMES,
        "the route kept {latency} frames of latency after its input stream ran \
         ahead of its output stream at start-up; it must settle to its own \
         cushion ({TARGET} frames) like a route whose streams started together"
    );
}

/// Defect 3: the IR cold-start cushion (#592) is a 512-frame silence prime
/// meant to cover the convolver's first callbacks — but it was also the
/// route's TARGET, so the guard kept every one of those 512 frames as
/// latency for the life of the chain: 11.6 ms at 44.1 kHz on every IR chain,
/// insert or not. Measured live on the owner's tail route: 576 queued frames
/// steady. The prime is for the start; once the route has proved a clean
/// window it must rest at its own cushion like any other route.
#[test]
fn an_ir_chain_sheds_its_cold_start_cushion_once_it_runs_clean() {
    let mut rig = Rig::with_runtime(runtime_for(chain(vec![cab("cab")])));
    for _ in 0..2_000 {
        rig.period(false);
    }
    let latency = rig.pulse_latency();
    assert!(
        latency <= TARGET + SLACK_FRAMES,
        "the IR route still carries {latency} frames after ~3 s of clean \
         running; the cold-start cushion must be shed down to the route's \
         own target ({TARGET} frames) once it is no longer needed"
    );
}

// ── The structural pass (#965): one resting cushion per route, from birth ──

/// Input then output, `periods` times, the way one device cycle runs on
/// macOS (both units on one HAL thread, input first). Every listed route is
/// drained once per period.
fn lockstep(runtime: &Arc<ChainRuntimeState>, periods: usize, frames: usize, routes: &[usize]) {
    let input = vec![0.0; frames * DEVICE_CHANNELS];
    let mut out = vec![0.0; frames * DEVICE_CHANNELS];
    for _ in 0..periods {
        process_input_f32(runtime, 0, &input, DEVICE_CHANNELS);
        for &route in routes {
            out.fill(0.0);
            process_output_f32(runtime, route, &mut out, DEVICE_CHANNELS);
        }
    }
}

/// Defect 4 (adversarial review of `bdece1fb6`): a convolver-fed route was
/// born with a 512-frame prime above its 128-frame rest, and the drift guard
/// then cut the excess ~186 ms later — a skip of live audio after every cold
/// start, every off-thread live edit and every DI render. A convolver-fed
/// route is born exactly at its rest; no fresh route is ever cut.
#[test]
fn no_fresh_route_is_cut_and_a_convolver_fed_one_is_born_at_rest() {
    // (name, chain, targets, frames each route rests at between periods)
    let cases: Vec<(&str, Chain, Vec<usize>, Vec<usize>)> = vec![
        ("plain", chain(vec![gain("amp")]), vec![128], vec![0]),
        ("cab", chain(vec![cab("cab")]), vec![128], vec![128]),
        (
            "insert then cab",
            chain(vec![insert("insert:1", "syn2-main"), cab("cab")]),
            vec![128, 64],
            vec![128, 0],
        ),
        (
            "cab then insert",
            chain(vec![cab("cab"), insert("insert:1", "syn2-main")]),
            vec![128, 64],
            vec![0, 64],
        ),
    ];
    for (name, owner, targets, rests) in cases {
        let rt = Arc::new(
            build_chain_runtime_state(&owner, 44_100.0, &targets, &registry())
                .expect("the chain must build"),
        );
        let routes: Vec<usize> = (0..targets.len()).collect();
        lockstep(&rt, 2_000, FRAMES, &routes);
        let loaded = rt.output_routes.load();
        for (route, target) in targets.iter().enumerate() {
            let buffer = &loaded[route].as_ref().expect("route is written").buffer;
            assert_eq!(
                buffer.latency_trims(),
                0,
                "{name}: route {route} had its audio cut by the drift guard — a \
                 fresh route must be born at its resting cushion, not above it"
            );
            assert_eq!(
                buffer.len(),
                rests[route],
                "{name}: route {route} (target {target}) rests at {} frames \
                 between periods",
                buffer.len()
            );
        }
    }
}

/// Defect 5: the DI loop renders its chain on a worker, stepped in lockstep
/// 256 frames at a time with no elastic targets (the engine default). With
/// the IR prime above the target, every render of an IR chain skipped 256
/// frames of the loop ~180 ms in, and the #785 hand-off between two renders
/// no longer lined up.
#[test]
fn a_di_render_of_an_ir_chain_is_never_cut() {
    const BLOCK: usize = 256;
    let rt = Arc::new(
        build_chain_runtime_state(&chain(vec![cab("cab")]), 44_100.0, &[], &registry())
            .expect("the chain must build"),
    );
    let silence = vec![0.0; BLOCK];
    let mut drain = vec![0.0; BLOCK * 2];
    for _ in 0..200 {
        process_input_f32(&rt, 0, &silence, 1);
        process_output_f32(&rt, 0, &mut drain, 2);
    }
    let loaded = rt.output_routes.load();
    let buffer = &loaded[0].as_ref().expect("route 0").buffer;
    assert_eq!(
        buffer.latency_trims(),
        0,
        "the DI render cut its own loop — a lockstep render has no stall to shed"
    );
}

/// Defect 7 (pre-existing, #953 + #85): a route on ANOTHER clock is held at
/// its cushion by the resampler's servo. The drift guard also trimmed it, so
/// after one stall the two fought: the servo refilled, the guard cut, every
/// fraction of a second, for the life of the route. The servo owns a
/// cross-rate route's level; the guard must leave it alone.
#[test]
fn the_drift_guard_leaves_a_cross_rate_route_to_its_servo() {
    let rates: HashMap<DeviceId, f32> =
        [(DeviceId(QUANTUM.into()), 48_000.0)].into_iter().collect();
    let rt = Arc::new(
        build_chain_runtime_state_with_device_rates(
            &chain(vec![gain("amp")]),
            44_100.0,
            &rates,
            &[128],
            &registry(),
        )
        .expect("the chain must build"),
    );
    let input = vec![0.0; FRAMES * DEVICE_CHANNELS];
    let mut out = vec![0.0; FRAMES * DEVICE_CHANNELS];
    // Walk both clocks in time: the input device ticks every 64/44100 s, the
    // output device every 64/48000 s. One stretch of the output stalls.
    let (t_in, t_out) = (FRAMES as f64 / 44_100.0, FRAMES as f64 / 48_000.0);
    let (mut next_in, mut next_out) = (0.0_f64, 0.0_f64);
    let mut outputs = 0usize;
    while next_in < 20.0 {
        if next_in <= next_out {
            process_input_f32(&rt, 0, &input, DEVICE_CHANNELS);
            next_in += t_in;
        } else {
            outputs += 1;
            let stalled = (3_000..3_004).contains(&outputs);
            if !stalled {
                out.fill(0.0);
                process_output_f32(&rt, 0, &mut out, DEVICE_CHANNELS);
            }
            next_out += t_out;
        }
    }
    let loaded = rt.output_routes.load();
    let buffer = &loaded[0].as_ref().expect("route 0").buffer;
    assert_eq!(
        buffer.latency_trims(),
        0,
        "the drift guard cut a cross-rate route whose level the #85 servo owns"
    );
}

/// Real-hardware finding (`issue_670_real_streams_no_xruns`,
/// `adding_a_cab_live_does_not_spiral`): a route whose output device is NOT
/// its producer's input device runs on another clock at the same nominal
/// rate, so the two drift and the ring slowly drains — any cushion only
/// delays the first underrun. The #592 IR cushion (512 frames) is what kept
/// that battery clean for its window; the lean cushion underran every ~6 s.
/// On the producer's own clock nothing drifts (measured clean on the owner's
/// Quantum under the same load), so only a route on ANOTHER clock keeps it.
#[test]
fn an_ir_route_on_another_clock_keeps_the_592_cushion() {
    let registry = vec![IoBinding {
        id: "guitarra-1".into(),
        name: "Two interfaces".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("coreaudio:blackhole".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("coreaudio:speakers".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    let rt = build_chain_runtime_state(&chain(vec![cab("cab")]), 48_000.0, &[128], &registry)
        .expect("the chain must build");
    let routes = rt.output_routes.load();
    let route = routes[0].as_ref().expect("route 0");
    assert_eq!(
        route.buffer.target_level(),
        512,
        "an IR route on another clock rests at the #592 cushion"
    );
    assert_eq!(route.buffer.len(), 512, "and is born there");
}
