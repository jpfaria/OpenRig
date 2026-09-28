//! #979 — hypothesis: the #967 insert bridge. The symptom began with v0.5.1,
//! whose #967 bridge made the loop's dry signal the SUM of every head feeding
//! the send and parked the return's fill. If any of that behaviour survives
//! in the insert path (7ed72ecd9 removed the `insert_bridge` module, but not
//! the cut it fed), one of these breaks on the owner's `rig:input-7`:
//!
//! - a head leaks into the other head's stream (the owner hears "several
//!   streams stacked");
//! - a head reaches the tail around the gear (dry + wet, a few ms apart);
//! - the return feeds back into the send (the pedals hear themselves; the
//!   measured output peak reached +10.1 dBFS);
//! - the send plays a head's buffer twice, alternates the two heads' buffers,
//!   or drops one (the #967 commit named "two E/S on one input alternate
//!   their buffers" as a bug it had);
//! - a route's fill grows over minutes of play (latency_trims rising and
//!   underruns climbing in 64-frame multiples until a rebuild);
//! - a switch or a scene edit clicks.
//!
//! Child module of `issue_979_two_head_rig_tests`: the topology (registry,
//! chain, runtimes, routes, pipelines) comes from there through `super::`.
//! The HAL cycle is the parent's: one IO thread, the input callback, every
//! output callback, then the #670 worker. Unlike the parent's `Hal`, every
//! route gets its own zeroed buffer each callback — each route is its own
//! output stream, and only the device (CoreAudio) sums streams that land on
//! the same channels (the LEI ZERO: nothing is mixed in our code). The SYN-2
//! is modelled as a unity device: what leaves on the send channel `[3]` this
//! cycle comes back on In 3/4 (`[2,3]`) on the next one.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::{
    pipelines, process_input_f32, process_output_f32, rig_chain, rig_registry, runtimes,
    written_routes, FRAMES, HD8_CHANNELS, RATE, ROUTES, TARGET,
};
use crate::runtime_graph::build_per_input_runtimes;
use crate::runtime_state::ChainRuntimeState;

/// `guitarra-1` on In 1.
const GUITAR_1: usize = 0;
/// `guitarra-2` on In 2.
const GUITAR_2: usize = 1;
/// `syn2-main` return: SYN-2 DI OUT L/R on In 3/4.
const RETURN: [usize; 2] = [2, 3];
/// `syn2-main` send: USB playback 4 (the HD 8 mixer sends it to Out 8).
const SEND: usize = 3;
/// HAL cycles in one second at 44.1 kHz / 64 frames.
const SECOND: usize = 44_100 / FRAMES;
/// −120 dBFS: nothing of a source that is not routed to a stream may reach it.
const SILENCE: f32 = 1e-6;
/// Volume invariant tolerance (`volume_invariants_tests.rs`).
const TOLERANCE: f32 = 1e-3;
/// Where a route may rest at the start of its output callback: its cushion
/// plus the one period the worker pushes after the output callbacks. The live
/// capture of the healthy chain read 64 and never above 128.
const REST_BOUND: usize = TARGET + FRAMES;
/// A step between two samples larger than this share of the route's steady
/// level is a click. The stream fade-in (FADE_IN_FRAMES = 128) climbs in
/// 1/128 steps.
const CLICK: f32 = 0.1;

/// What comes back on In 3/4.
#[derive(Clone, Copy)]
enum Syn2 {
    /// The SYN-2 plays this constant level on its own (its DI OUT is live
    /// whether or not the chain's loop is on).
    Level(f32),
    /// The physical loop: the send channel comes back at unity one cycle later.
    Loop,
}

/// The owner's HD 8 driving the chain's runtimes, one HAL cycle at a time.
struct Rig {
    runtimes: Vec<Arc<ChainRuntimeState>>,
    /// (runtime, route, channels) of every route the chain writes.
    routes: Vec<(usize, usize, Vec<usize>)>,
    input: Vec<f32>,
    /// One device buffer per route: each route is its own output stream.
    outs: Vec<Vec<f32>>,
    /// The physical send of the last cycle, frame by frame: what the SYN-2
    /// answers with on the next one.
    gear: Vec<f32>,
    /// Fill of each route at the start of its last output callback.
    fills: Vec<usize>,
}

impl Rig {
    fn new(runtimes: Vec<Arc<ChainRuntimeState>>) -> Self {
        let routes: Vec<(usize, usize, Vec<usize>)> = runtimes
            .iter()
            .enumerate()
            .flat_map(|(i, rt)| {
                written_routes(rt)
                    .into_iter()
                    .map(move |(route, channels)| (i, route, channels))
            })
            .collect();
        let count = routes.len();
        Self {
            runtimes,
            routes,
            input: vec![0.0; FRAMES * HD8_CHANNELS],
            outs: vec![vec![0.0; FRAMES * HD8_CHANNELS]; count],
            gear: vec![0.0; FRAMES],
            fills: vec![0; count],
        }
    }

    /// One HAL cycle: the guitars at `g1` / `g2` (constant), the SYN-2 as
    /// `syn2`; every output callback; then the worker, on time.
    fn cycle(&mut self, g1: f32, g2: f32, syn2: Syn2) {
        for (f, frame) in self.input.chunks_exact_mut(HD8_CHANNELS).enumerate() {
            frame[GUITAR_1] = g1;
            frame[GUITAR_2] = g2;
            let back = match syn2 {
                Syn2::Level(level) => level,
                Syn2::Loop => self.gear[f],
            };
            frame[RETURN[0]] = back;
            frame[RETURN[1]] = back;
        }
        for (i, (rt, route, _)) in self.routes.iter().enumerate() {
            let runtime = &self.runtimes[*rt];
            self.fills[i] = fill(runtime, *route);
            self.outs[i].fill(0.0);
            process_output_f32(runtime, *route, &mut self.outs[i], HD8_CHANNELS);
        }
        // The device sums every stream on the send channel; the SYN-2 hears it.
        self.gear.fill(0.0);
        for (i, (_, _, channels)) in self.routes.iter().enumerate() {
            if channels.as_slice() == [SEND] {
                for (f, g) in self.gear.iter_mut().enumerate() {
                    *g += self.outs[i][f * HD8_CHANNELS + SEND];
                }
            }
        }
        for rt in &self.runtimes {
            process_input_f32(rt, 0, &self.input, HD8_CHANNELS);
        }
    }

    fn run(&mut self, cycles: usize, g1: f32, g2: f32, syn2: Syn2) {
        for _ in 0..cycles {
            self.cycle(g1, g2, syn2);
        }
    }

    /// Plays `cycles` and returns every route's first channel, frame by frame.
    fn play(&mut self, cycles: usize, g1: f32, g2: f32, syn2: Syn2) -> Vec<Vec<f32>> {
        let mut takes = vec![Vec::with_capacity(cycles * FRAMES); self.routes.len()];
        for _ in 0..cycles {
            self.cycle(g1, g2, syn2);
            for (i, take) in takes.iter_mut().enumerate() {
                take.extend(self.route_frames(i));
            }
        }
        takes
    }

    /// Route `i`'s first channel in this cycle's buffer.
    fn route_frames(&self, i: usize) -> impl Iterator<Item = f32> + '_ {
        let channel = self.routes[i].2[0];
        self.outs[i]
            .chunks_exact(HD8_CHANNELS)
            .map(move |frame| frame[channel])
    }

    /// Indices (into `routes`) of the routes on the send channel.
    fn sends(&self) -> Vec<usize> {
        (0..self.routes.len())
            .filter(|&i| self.routes[i].2.as_slice() == [SEND])
            .collect()
    }

    /// Indices (into `routes`) of every route that is not the send.
    fn tails(&self) -> Vec<usize> {
        (0..self.routes.len())
            .filter(|&i| self.routes[i].2.as_slice() != [SEND])
            .collect()
    }

    /// Indices (into `routes`) of the routes written by the pipelines that
    /// read the head on `input`.
    fn head_routes(&self, input: usize) -> Vec<usize> {
        let mut owned: Vec<(usize, usize)> = Vec::new();
        for (ri, rt) in self.runtimes.iter().enumerate() {
            for (inputs, routes) in pipelines(rt) {
                if inputs == vec![input] {
                    owned.extend(routes.into_iter().map(|route| (ri, route)));
                }
            }
        }
        (0..self.routes.len())
            .filter(|&i| owned.contains(&(self.routes[i].0, self.routes[i].1)))
            .collect()
    }

    /// (underrun frames, latency trims) of every route.
    fn counters(&self) -> Vec<(u64, u64)> {
        self.routes
            .iter()
            .map(|(rt, route, _)| {
                self.runtimes[*rt]
                    .output_routes
                    .load()
                    .get(*route)
                    .and_then(|r| r.as_ref())
                    .map_or((0, 0), |r| {
                        (r.buffer.underrun_count(), r.buffer.latency_trims())
                    })
            })
            .collect()
    }
}

fn fill(runtime: &ChainRuntimeState, route: usize) -> usize {
    runtime
        .output_routes
        .load()
        .get(route)
        .and_then(|r| r.as_ref())
        .map_or(0, |r| r.buffer.len())
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
}

fn mean_abs(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s.abs()).sum::<f32>() / samples.len().max(1) as f32
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "both runs must play the same frames");
    a.iter()
        .zip(b)
        .fold(0.0_f32, |m, (x, y)| m.max((x - y).abs()))
}

/// (frame, size) of the largest sample-to-sample step, counting the step in
/// from the silence before the route existed.
fn max_step(samples: &[f32]) -> (usize, f32) {
    let mut prev = 0.0_f32;
    let mut worst = (0usize, 0.0_f32);
    for (i, &s) in samples.iter().enumerate() {
        let step = (s - prev).abs();
        if step > worst.1 {
            worst = (i, step);
        }
        prev = s;
    }
    worst
}

/// Frame-by-frame sum of the given takes: what the device plays on a channel
/// several streams share.
fn sum_of(takes: &[Vec<f32>], indices: &[usize]) -> Vec<f32> {
    let frames = takes.first().map_or(0, |t| t.len());
    (0..frames)
        .map(|f| indices.iter().map(|&i| takes[i][f]).sum())
        .collect()
}

/// Insert off, the chain is two isolated pipelines, one per guitar. Nothing
/// another source does — the other guitar playing, the SYN-2 sending on In
/// 3/4 while its loop is switched off — may change a single sample on a
/// head's own routes (invariant #4: N streams = N pipelines).
#[test]
fn insert_off_no_other_source_changes_a_heads_routes() {
    for (head, alone, crowded) in [
        (GUITAR_1, (0.1, 0.0, 0.0), (0.1, 0.3, 0.5)),
        (GUITAR_2, (0.0, 0.1, 0.0), (0.3, 0.1, 0.5)),
    ] {
        let run = |(g1, g2, syn2): (f32, f32, f32)| {
            let mut rig = Rig::new(runtimes(false));
            let own = rig.head_routes(head);
            let takes = rig.play(SECOND, g1, g2, Syn2::Level(syn2));
            own.iter()
                .map(|&i| (rig.routes[i].1, takes[i].clone()))
                .collect::<Vec<_>>()
        };
        let alone = run(alone);
        let crowded = run(crowded);
        assert!(
            !alone.is_empty(),
            "the guitar on In {} owns no route",
            head + 1
        );
        for ((route, a), (_, c)) in alone.iter().zip(&crowded) {
            assert!(
                peak(a) > 0.01,
                "the guitar on In {} must reach its own route {route}",
                head + 1
            );
            let diff = max_diff(a, c);
            assert!(
                diff < SILENCE,
                "insert off: route {route} of the guitar on In {} moved by up to {diff} \
                 when the other guitar and the SYN-2 played — another source leaked into \
                 this head's stream",
                head + 1
            );
        }
    }
}

/// Insert on, the loop is a cut: the heads reach the tail ONLY through the
/// SYN-2. A dry path around the gear — the #967 bridge's "dry = sum of every
/// head" left audible next to the return — plays the guitar twice, once dry
/// and once through the pedals, a few ms apart: the stacked sound the owner
/// hears. With the SYN-2 answering a fixed signal, the tail routes must not
/// move by a single sample whether the guitars play or not.
#[test]
fn insert_on_the_tails_hear_only_the_return() {
    let run = |guitars: f32| {
        let mut rig = Rig::new(runtimes(true));
        let tails = rig.tails();
        let takes = rig.play(SECOND, guitars, guitars, Syn2::Level(0.3));
        tails
            .iter()
            .map(|&i| (rig.routes[i].1, takes[i].clone()))
            .collect::<Vec<_>>()
    };
    let quiet = run(0.0);
    let playing = run(0.1);
    assert!(!quiet.is_empty(), "the return must feed the chain's tail");
    for ((route, q), (_, p)) in quiet.iter().zip(&playing) {
        assert!(peak(q) > 0.01, "the return must reach tail route {route}");
        let diff = max_diff(q, p);
        assert!(
            diff < SILENCE,
            "insert on: tail route {route} moved by up to {diff} when the guitars \
             played — a head reaches the output around the SYN-2"
        );
    }
}

/// Insert on: the send carries the heads and nothing else. A return-to-send
/// path closes the loop through the pedals inside our code — the gear hears
/// its own output, which rings and builds up (measured peak: +10.1 dBFS).
/// With the guitars fixed, the physical send must not move by a single
/// sample whether the SYN-2 returns silence or a loud signal.
#[test]
fn insert_on_the_send_hears_only_the_heads() {
    let run = |syn2: f32| {
        let mut rig = Rig::new(runtimes(true));
        let sends = rig.sends();
        let takes = rig.play(SECOND, 0.1, 0.1, Syn2::Level(syn2));
        sum_of(&takes, &sends)
    };
    let dry = run(0.0);
    let loud = run(0.5);
    assert!(peak(&dry) > 0.01, "the heads must reach the send");
    let diff = max_diff(&dry, &loud);
    assert!(
        diff < SILENCE,
        "insert on: the SYN-2's return moved the send by up to {diff} — the return \
         feeds back into the send"
    );
}

/// The physical loop closed at unity: once the guitars stop, what is in
/// flight (the rings' cushions, the gear's one-cycle hop) comes out within a
/// quarter second, and then every route — send and tails — is silent for
/// good. A route still sounding seconds later is audio recirculating through
/// our code: the "loop" the owner hears.
#[test]
fn insert_on_the_closed_loop_rings_out_when_the_guitars_stop() {
    let mut rig = Rig::new(runtimes(true));
    let played = rig.play(2 * SECOND, 0.1, 0.1, Syn2::Loop);
    for &i in &rig.tails() {
        let take = &played[i];
        assert!(
            peak(&take[take.len() / 2..]) > 0.01,
            "route {} {:?}: the guitars must come back through the SYN-2",
            rig.routes[i].1,
            rig.routes[i].2
        );
    }
    rig.run(SECOND / 4, 0.0, 0.0, Syn2::Loop);
    let after = rig.play(5 * SECOND, 0.0, 0.0, Syn2::Loop);
    for (i, take) in after.iter().enumerate() {
        let p = peak(take);
        assert!(
            p < SILENCE,
            "route {} {:?} still carries {p} up to 5 s after the guitars stopped — \
             the loop recirculates",
            rig.routes[i].1,
            rig.routes[i].2
        );
    }
}

/// Insert on, both heads feed the one send (`[3]`, mono). The device plays
/// on that channel exactly g1 + g2 on every frame: each head's buffer once
/// (not twice), never one head's buffer in place of the other's (the
/// alternation the #967 commit fixed once), never a dropped buffer. A mono
/// head into a mono send is unity (volume pin a02).
#[test]
fn insert_on_the_send_carries_every_frame_of_each_head_exactly_once() {
    for (g1, g2) in [(0.1_f32, 0.0_f32), (0.0, 0.1), (0.1, 0.03)] {
        let mut rig = Rig::new(runtimes(true));
        let sends = rig.sends();
        assert!(
            !sends.is_empty(),
            "insert on: something must write the send"
        );
        // Start-up: the fade-in and the routes' first callbacks.
        rig.run(SECOND, g1, g2, Syn2::Level(0.0));
        let takes = rig.play(2 * SECOND, g1, g2, Syn2::Level(0.0));
        let send = sum_of(&takes, &sends);
        let want = g1 + g2;
        let wrong: Vec<(usize, f32)> = send
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, s)| (s - want).abs() > TOLERANCE)
            .collect();
        assert!(
            wrong.is_empty(),
            "insert on, guitars {g1}/{g2}: {} of {} send frames are not {want} (first: \
             frame {} = {}) — a head's buffer went out twice, alternated with the other \
             head's, or was dropped",
            wrong.len(),
            send.len(),
            wrong[0].0,
            wrong[0].1
        );
    }
}

/// Five minutes of steady play with the DSP worker always on time (both
/// workers were only ~28% busy on the rig), loop closed when the insert is
/// on. No route may ever rest above its cushion plus one period, lose a
/// frame, or be trimmed: the rig showed latency_trims rising and underruns
/// climbing in 64-frame multiples until a rebuild, one route far faster than
/// its sibling (12032 vs 2496 underruns). The return-fed tails are the
/// #967 "parked return fill": it must stay at about one period for good.
#[test]
fn every_route_rests_within_one_period_through_five_minutes_of_play() {
    const MINUTES: usize = 5;
    for insert_enabled in [false, true] {
        let mut rig = Rig::new(runtimes(insert_enabled));
        let syn2 = if insert_enabled {
            Syn2::Loop
        } else {
            Syn2::Level(0.0)
        };
        // Start-up: the routes are born empty, so the first callback underruns.
        rig.run(SECOND, 0.1, 0.1, syn2);
        let before = rig.counters();
        let mut max_fill = vec![0usize; rig.routes.len()];
        for _ in 0..MINUTES * 60 * SECOND {
            rig.cycle(0.1, 0.1, syn2);
            for (m, f) in max_fill.iter_mut().zip(&rig.fills) {
                *m = (*m).max(*f);
            }
        }
        for (i, ((u, t), (u0, t0))) in rig.counters().into_iter().zip(before).enumerate() {
            let (route, channels) = (rig.routes[i].1, &rig.routes[i].2);
            assert!(
                max_fill[i] <= REST_BOUND && u == u0 && t == t0,
                "insert {insert_enabled}, route {route} {channels:?}: over {MINUTES} min \
                 with the worker always on time the route rose to {} queued frames \
                 (bound {REST_BOUND}) with {} underrun frames and {} latency trims — its \
                 latency grew with nothing late",
                max_fill[i],
                u - u0,
                t - t0
            );
        }
    }
}

/// On this rig an insert switch regroups the chain (one runtime with the
/// loop on, one per guitar with it off, `chain_structure_signature`), so the
/// switch lands on FRESH runtimes. Each one must bring every route in on a
/// ramp: a route that jumps from silence to its level clicks on every scene
/// change that touches the SYN-2.
#[test]
fn a_switch_lands_on_runtimes_that_enter_every_route_without_a_click() {
    for insert_enabled in [false, true] {
        let mut rig = Rig::new(runtimes(insert_enabled));
        let syn2 = if insert_enabled {
            Syn2::Loop
        } else {
            Syn2::Level(0.0)
        };
        let takes = rig.play(SECOND, 0.1, 0.1, syn2);
        for (i, take) in takes.iter().enumerate() {
            let (route, channels) = (rig.routes[i].1, &rig.routes[i].2);
            let steady = mean_abs(&take[take.len() - 256..]);
            assert!(
                steady > 0.01,
                "insert {insert_enabled}, route {route} {channels:?} must play"
            );
            let (at, step) = max_step(take);
            assert!(
                step <= steady * CLICK,
                "insert {insert_enabled}, route {route} {channels:?}: a {step} step at \
                 frame {at} against a steady {steady} — the route came in without a ramp"
            );
        }
    }
}

/// A volume block at 100% (unity) standing in for the NAM preamp / IR cab,
/// so bypassing it changes nothing audible.
fn unity(id: &str, enabled: bool) -> AudioBlock {
    let schema = schema_for_block_model("gain", "volume").expect("the volume block has a schema");
    let mut params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults must normalize");
    params.insert("volume", ParameterValue::Float(100.0));
    params.insert("mute", ParameterValue::Bool(false));
    AudioBlock {
        id: BlockId(id.into()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params,
        }),
    }
}

/// `rig:input-7` with the insert on and unity blocks; `cab_on` is the scene.
fn scene(cab_on: bool) -> Chain {
    let mut chain = rig_chain(true);
    chain.blocks = chain
        .blocks
        .into_iter()
        .map(|block| match block.id.0.as_str() {
            "preamp" => unity("preamp", true),
            "cab" => unity("cab", cab_on),
            _ => block,
        })
        .collect();
    chain
}

fn build(chain: &Chain) -> Vec<Arc<ChainRuntimeState>> {
    build_per_input_runtimes(
        chain,
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

/// The owner's chain holds two VST3 blocks, so a scene edit is applied IN
/// PLACE (#779) on the running runtime while the loop plays. A scene whose
/// only change is bypassing a unity block changes nothing audible, so the
/// edit must not be heard at all: every route keeps its level on every frame
/// (no gap, no dip, no doubled buffer), loses no frame, is not trimmed and
/// does not rest any deeper.
#[test]
fn insert_on_an_in_place_scene_edit_is_inaudible() {
    let mut rig = Rig::new(build(&scene(true)));
    assert_eq!(rig.runtimes.len(), 1, "an insert chain is one runtime");
    let warm = rig.play(2 * SECOND, 0.1, 0.1, Syn2::Loop);
    let steady: Vec<f32> = warm
        .iter()
        .map(|take| mean_abs(&take[take.len() - 256..]))
        .collect();
    let before = rig.counters();

    crate::runtime_graph_update::update_chain_runtime_state(
        &rig.runtimes[0],
        &scene(false),
        RATE,
        false,
        &[TARGET; ROUTES],
        &rig_registry(),
    )
    .expect("the in-place scene edit must apply");
    assert_eq!(
        written_routes(&rig.runtimes[0]),
        rig.routes
            .iter()
            .map(|(_, route, channels)| (*route, channels.clone()))
            .collect::<Vec<_>>(),
        "a scene edit that only bypasses a block keeps every route"
    );

    let mut max_fill = vec![0usize; rig.routes.len()];
    let mut takes: Vec<Vec<f32>> = vec![Vec::new(); rig.routes.len()];
    for _ in 0..2 * SECOND {
        rig.cycle(0.1, 0.1, Syn2::Loop);
        for (i, take) in takes.iter_mut().enumerate() {
            take.extend(rig.route_frames(i));
            max_fill[i] = max_fill[i].max(rig.fills[i]);
        }
    }
    for (i, ((u, t), (u0, t0))) in rig.counters().into_iter().zip(before).enumerate() {
        let (route, channels) = (rig.routes[i].1, &rig.routes[i].2);
        assert!(
            steady[i] > 0.01,
            "route {route} {channels:?} must play before the edit"
        );
        let (lo, hi) = (steady[i] * (1.0 - CLICK), steady[i] * (1.0 + CLICK));
        let heard = takes[i]
            .iter()
            .enumerate()
            .find(|(_, s)| s.abs() < lo || s.abs() > hi);
        assert!(
            heard.is_none(),
            "route {route} {channels:?}: frame {:?} after the edit left the steady level \
             {} — the edit was heard (a gap, a dip or a doubled buffer)",
            heard,
            steady[i]
        );
        assert!(
            max_fill[i] <= REST_BOUND && u == u0 && t == t0,
            "route {route} {channels:?}: after the in-place edit the route rose to {} \
             queued frames (bound {REST_BOUND}) with {} underrun frames and {} latency \
             trims",
            max_fill[i],
            u - u0,
            t - t0
        );
    }
}
