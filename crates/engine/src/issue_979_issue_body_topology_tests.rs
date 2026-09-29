//! #979 — the surviving RED families rerun on the topology the issue body
//! measured, not the symmetric one the rest of the suite builds.
//!
//! The issue body: `guitarra-1` (In 1) goes to Main `[0,1]` and to the ADAT
//! pair `[24,25]`; `guitarra-2` (In 2) goes to Main `[0,1]` and to `[10,11]`;
//! the `syn2-main` insert sends on `[3]` and returns on In 3/4 `[2,3]`. The
//! measured routes were `[0,1]` twice (routes 0 and 2), `[24,25]`, `[10,11]`
//! and `[3]`. The parent's `rig_registry` gives both heads `[10,11]`, so every
//! test of rounds 1 and 2 ran on `[0,1]`×2 + `[10,11]`×2 (critic item 8).
//!
//! Hypothesis: the surviving REDs are not artifacts of that symmetric
//! registry. The route-local damage — the spiral a disturbance leaves in a
//! route's #953 guard, a late buffer that keeps costing after the worker is
//! back on time, siblings of one stream that rest or pay differently — does
//! not depend on which physical channels a route writes, so it shows on this
//! topology the same way, with the insert on and off. The doubled insert
//! return lands only where two routes share a physical output: here on Main
//! alone, while `[24,25]` and `[10,11]` are reached by one head each. A fix
//! that keeps the return on one head's routes only would silence the other
//! head's own output on this rig, which the symmetric rig cannot show.
//!
//! Intended behaviour is identical to the symmetric case (the owner's
//! invariants): N streams are N isolated pipelines and nothing is summed in
//! our code; a disturbance costs at most the buffers it took away and
//! nothing once the worker and the device are on time; latency never grows
//! and routes of one stream stay in step; per-stream volume is immutable, so
//! every physical output plays each source once, at the level a one-E/S rig
//! plays it there.
//!
//! Child module of `issue_979_two_head_rig_tests`: the chain, the entry points
//! and the rig constants come from there through `super::`; only the
//! registry differs. The HAL cycle is the parent's (#965): the input
//! callback, every started output unit in route order, then each runtime's
//! #670 worker. On top of it this harness lets a worker be late, catch up
//! between two output callbacks, and lets the HD 8 lose, defer or repeat the
//! input callback, skip or repeat every output unit, stall one output stream,
//! and start the output streams one after another.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use domain::io_binding::IoBinding;
use project::chain::Chain;

use super::{
    pipelines, process_input_f32, process_output_f32, rig_chain, rig_registry, written_routes,
    FRAMES, HD8_CHANNELS, RATE, ROUTES, TARGET,
};
use crate::runtime_graph::build_per_input_runtimes;
use crate::runtime_state::ChainRuntimeState;

/// Main L/R (Out 1/2): both heads reach it.
const MAIN: [usize; 2] = [0, 1];
/// The ADAT pair only `guitarra-1` reaches ("guitarra-1 also -> 24/25").
const ADAT: [usize; 2] = [24, 25];
/// The pair only `guitarra-2` reaches ("guitarra-2 also -> 10/11").
const OUT2: [usize; 2] = [10, 11];
/// The `syn2-main` send: USB playback 4, mono.
const SEND: usize = 3;
/// Every physical output the chain's tail reaches: Main L/R, ADAT L/R,
/// Out `[10,11]` L/R.
const TAIL: [usize; 6] = [0, 1, 24, 25, 10, 11];
/// HD 8 inputs: the two mono guitars and the SYN-2's stereo return.
const GUITAR_1: usize = 0;
const GUITAR_2: usize = 1;
const RETURN_L: usize = 2;
const RETURN_R: usize = 3;
const SOURCES: [usize; 4] = [GUITAR_1, GUITAR_2, RETURN_L, RETURN_R];
/// A steady guitar, the parent's level.
const LEVEL: f32 = 0.1;
const PULSE: f32 = 0.5;
/// A sample above this is heard.
const HEARD: f32 = 1e-3;
/// Routes of a one-E/S rig: its Main, its second output and the send.
const ONE_E_S_ROUTES: usize = 3;

/// Callbacks per #953 guard window: 8192 frames (~186 ms) at 64 frames.
const WINDOW: usize = 8_192 / FRAMES;
/// HAL cycles in one simulated minute: 44 100 Hz × 60 s / 64 frames.
const MINUTE: usize = 44_100 * 60 / FRAMES;
const TEN_SECONDS: usize = MINUTE / 6;
/// Two simulated minutes of disturbances. #991: pre-#979 damage never decays,
/// so every run of the decay test is already red by then; the eight minutes
/// after it only made Coverage time out.
const HISTORY: usize = 2 * MINUTE;
/// On-time cycles past the cold start (the routes are born empty). ~1.5 s.
const WARM_UP: usize = 1_000;
/// What the guard gets to shed what a disturbance left behind (~560 ms).
const SETTLE: usize = 3 * WINDOW;
/// Idle cycles before a pulse is timed (8 guard windows, ~1.5 s): long enough
/// for every route to shed what the guard is meant to shed.
const IDLE: usize = 1_024;
/// Cycles a pulse is listened for.
const LISTEN: usize = 64;
/// One late worker push every ~1.4 s (a busy machine), coprime with the
/// window, so the pushes land on every window position.
const LATE_EVERY: usize = 997;
/// #991: one lap, each window position once. A second lap over the same
/// positions doubled the run.
const LATE_EVENTS: usize = WINDOW;
/// Output streams starting one HAL cycle apart, in route order (by route).
const CAPTURE_ORDER: [usize; ROUTES] = [0, 1, 2, 3, 4];
/// Cycles for a level change to cross every route.
const LEVEL_SETTLE: usize = 64;
/// Cycles a level is measured over (~0.37 s).
const MEASURE: usize = 256;
/// "The same level": an extra copy of a signal is +6 dB aligned, +3 dB a
/// buffer apart; a missing one is silence.
const SAME_DB: f32 = 0.5;

/// The owner's bindings as the issue body lists them: the parent's
/// `rig_registry` with `guitarra-1`'s second output on the ADAT pair
/// `[24,25]` instead of `[10,11]`. Nothing else differs from the symmetric rig.
fn issue_body_registry() -> Vec<IoBinding> {
    let mut registry = rig_registry();
    assert_eq!(
        (
            registry[0].inputs[0].channels.clone(),
            registry[0].outputs[1].channels.clone(),
            registry[1].inputs[0].channels.clone(),
            registry[1].outputs[1].channels.clone(),
        ),
        (vec![GUITAR_1], OUT2.to_vec(), vec![GUITAR_2], OUT2.to_vec()),
        "precondition: the parent's registry is guitarra-1 on In 1 and guitarra-2 on In 2, \
         each with its second output on [10,11]"
    );
    registry[0].outputs[1].channels = ADAT.to_vec();
    registry[0].outputs[1].name = "ADAT [24,25]".into();
    registry
}

fn build(chain: &Chain, routes: usize) -> Vec<Arc<ChainRuntimeState>> {
    build_per_input_runtimes(
        chain,
        RATE,
        &HashMap::new(),
        &vec![TARGET; routes],
        &issue_body_registry(),
    )
    .expect("the issue body's chain must build")
    .into_iter()
    .map(|(_, state)| Arc::new(state))
    .collect()
}

fn silence() -> Vec<f32> {
    vec![0.0; FRAMES * HD8_CHANNELS]
}

fn db(ratio: f32) -> f32 {
    20.0 * ratio.max(1e-12).log10()
}

/// Two levels match within ~0.1 dB (or both are silent).
fn same_level(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-5 + 0.012 * a.abs().max(b.abs())
}

/// When a runtime's #670 worker pushes, relative to one HAL cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Push {
    /// After this cycle's output callbacks: the normal hand-off.
    OnTime,
    /// Not before the next cycle's output callbacks.
    Late,
    /// A late worker catching up after the first `n` output callbacks of this
    /// cycle; this cycle's own buffer still lands after them all.
    Midway(usize),
}

/// What the HD 8's IO thread runs in one cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Io {
    /// Input callbacks: 0 when the HAL lost or deferred it, 2 or more when it
    /// delivers deferred input back to back.
    inputs: usize,
    /// Times every output unit is called: 0 when the HAL skipped them, 2 when
    /// it catches up.
    passes: usize,
}

const NORMAL: Io = Io {
    inputs: 1,
    passes: 1,
};
/// The IO thread missed the cycle: no unit of the device ran.
const IO_STALLED: Io = Io {
    inputs: 0,
    passes: 0,
};
/// The input callback did not run; every output unit did.
const NO_INPUT: Io = Io {
    inputs: 0,
    passes: 1,
};
/// The input callback ran; no output unit did.
const OUTPUTS_SKIPPED: Io = Io {
    inputs: 1,
    passes: 0,
};

/// One CoreAudio output unit of the HD 8: one route of the chain.
struct Unit {
    runtime: usize,
    route: usize,
    channels: Vec<usize>,
    /// Cycle of the unit's first callback, counted from the input's first.
    starts_at: usize,
    /// Frames queued at the start of its last callback.
    fill: usize,
    /// Output callbacks it has run since the build.
    callbacks: usize,
    /// First frame of this cycle it played audibly on its channels.
    heard: Option<usize>,
}

/// What one lateness event cost a route.
struct Outcome {
    /// Underrun frames from the first late cycle through the catch-up.
    lost_while_late: u64,
    /// Underrun frames once the worker was back on time.
    lost_after: u64,
    /// Trims from the first late cycle until the route had settled.
    trims_to_settle: u64,
    /// Trims once settled.
    trims_after: u64,
    /// Frames queued at the route's last callback start once settled.
    settled_fill: usize,
}

/// The issue body's chain on one simulated Quantum HD 8.
struct Rig {
    runtimes: Vec<Arc<ChainRuntimeState>>,
    /// Every output unit, in callback order.
    units: Vec<Unit>,
    /// Per runtime (one #670 worker each): input buffers handed over and not
    /// pushed yet, oldest first.
    pending: Vec<VecDeque<Vec<f32>>>,
    /// What the next input callback delivers, all 30 channels interleaved.
    input: Vec<f32>,
    /// One unit's output callback buffer.
    out: Vec<f32>,
    /// What the HD 8 played this cycle: every unit's callback summed per
    /// channel (the backend's mix, not ours). Only summed while something
    /// reads it (`listening` or `gear_in_the_loop`).
    played: Vec<f32>,
    /// A `listen` is running. #991: the long simulations never read `played`,
    /// and summing 30 channels per callback for nothing dominated their cost
    /// under llvm-cov.
    listening: bool,
    /// A unit whose output stream misses its callbacks while set.
    stalled: Option<usize>,
    /// The SYN-2 at unity: what left on the send comes back on both return
    /// channels in the next input callback.
    gear_in_the_loop: bool,
    /// HAL cycles since the build.
    now: usize,
}

impl Rig {
    fn from_runtimes(runtimes: Vec<Arc<ChainRuntimeState>>) -> Self {
        let units: Vec<Unit> = runtimes
            .iter()
            .enumerate()
            .flat_map(|(rt, runtime)| {
                written_routes(runtime)
                    .into_iter()
                    .map(move |(route, channels)| Unit {
                        runtime: rt,
                        route,
                        channels,
                        starts_at: 0,
                        fill: 0,
                        callbacks: 0,
                        heard: None,
                    })
            })
            .collect();
        let mut input = silence();
        for frame in input.chunks_exact_mut(HD8_CHANNELS) {
            for ch in SOURCES {
                frame[ch] = LEVEL;
            }
        }
        Self {
            pending: runtimes.iter().map(|_| VecDeque::new()).collect(),
            runtimes,
            units,
            input,
            out: silence(),
            played: silence(),
            listening: false,
            stalled: None,
            gear_in_the_loop: false,
            now: 0,
        }
    }

    /// The owner's chain (`guitarra-1` + `guitarra-2` + `syn2-main`) on the
    /// issue body's bindings, every output stream up with the input.
    fn new(insert_enabled: bool) -> Self {
        let rig = Self::from_runtimes(build(&rig_chain(insert_enabled), ROUTES));
        let mut reached: Vec<Vec<usize>> = rig.units.iter().map(|u| u.channels.clone()).collect();
        reached.sort();
        reached.dedup();
        let mut expected = vec![MAIN.to_vec(), ADAT.to_vec(), OUT2.to_vec()];
        if insert_enabled {
            expected.push(vec![SEND]);
        }
        expected.sort();
        assert_eq!(
            reached, expected,
            "precondition, insert {insert_enabled}: the chain's routes reach the issue body's \
             outputs ([0,1], [24,25], [10,11], and the send [3] with the insert on)"
        );
        rig
    }

    /// The same chain on one E/S alone: the level every signal has when it
    /// is played exactly once.
    fn one_e_s(insert_enabled: bool, binding: &str) -> Self {
        let mut chain = rig_chain(insert_enabled);
        chain.io_binding_ids = vec![binding.into()];
        Self::from_runtimes(build(&chain, ONE_E_S_ROUTES))
    }

    /// The output streams come up `by_route[route]` cycles after the input
    /// (a missing entry starts with it).
    fn starting(insert_enabled: bool, by_route: &[usize]) -> Self {
        let mut rig = Self::new(insert_enabled);
        for unit in &mut rig.units {
            unit.starts_at = by_route.get(unit.route).copied().unwrap_or(0);
        }
        rig
    }

    /// Played on time past the cold start. Precondition of every test that
    /// starts from it: with the worker and the device on time the chain runs
    /// clean, so what a test then measures is the disturbance's doing.
    fn warm(insert_enabled: bool) -> Self {
        let mut rig = Self::new(insert_enabled);
        rig.run(WARM_UP);
        let warm = rig.counters();
        rig.run(2 * WINDOW);
        for (i, (now, then)) in rig.counters().iter().zip(&warm).enumerate() {
            assert_eq!(
                (now.0 - then.0, now.1 - then.1),
                (0, 0),
                "precondition, issue-body rig, insert {insert_enabled}: {} underran or was \
                 trimmed with the worker and the device on time (underrun frames, trims)",
                rig.name(i)
            );
        }
        rig
    }

    fn warmed(mut self) -> Self {
        self.run(WARM_UP);
        self
    }

    /// Plays on time until every unit has started and is past its cold start.
    fn warm_up(&mut self) {
        let last_start = self.units.iter().map(|u| u.starts_at).max().unwrap_or(0);
        self.run(last_start + WARM_UP);
    }

    fn name(&self, i: usize) -> String {
        format!("route {} {:?}", self.units[i].route, self.units[i].channels)
    }

    fn all(&self, push: Push) -> Vec<Push> {
        vec![push; self.runtimes.len()]
    }

    fn queued(&self, i: usize) -> usize {
        let unit = &self.units[i];
        self.runtimes[unit.runtime].output_routes.load()[unit.route]
            .as_ref()
            .expect("a route the chain writes")
            .buffer
            .len()
    }

    /// (underrun frames, latency trims) of every unit, in callback order.
    fn counters(&self) -> Vec<(u64, u64)> {
        self.units
            .iter()
            .map(|unit| {
                let loaded = self.runtimes[unit.runtime].output_routes.load();
                let buffer = &loaded[unit.route]
                    .as_ref()
                    .expect("a route the chain writes")
                    .buffer;
                (buffer.underrun_count(), buffer.latency_trims())
            })
            .collect()
    }

    /// Frames queued at the start of each unit's last callback.
    fn fills(&self) -> Vec<usize> {
        self.units.iter().map(|u| u.fill).collect()
    }

    /// Worker `rt` pushes the buffers it holds, oldest first, leaving the
    /// newest `keep`.
    fn work(&mut self, rt: usize, keep: usize) {
        while self.pending[rt].len() > keep {
            if let Some(buffer) = self.pending[rt].pop_front() {
                process_input_f32(&self.runtimes[rt], 0, &buffer, HD8_CHANNELS);
            }
        }
    }

    fn play_unit(&mut self, i: usize) {
        let fill = self.queued(i);
        let (rt, route) = (self.units[i].runtime, self.units[i].route);
        self.out.fill(0.0);
        process_output_f32(&self.runtimes[rt], route, &mut self.out, HD8_CHANNELS);
        if self.listening || self.gear_in_the_loop {
            for (sum, sample) in self.played.iter_mut().zip(&self.out) {
                *sum += *sample;
            }
        }
        let out = &self.out;
        let unit = &mut self.units[i];
        unit.fill = fill;
        unit.callbacks += 1;
        if unit.heard.is_none() {
            unit.heard = out
                .chunks_exact(HD8_CHANNELS)
                .position(|frame| unit.channels.iter().any(|&ch| frame[ch].abs() > HEARD));
        }
    }

    /// One HAL cycle: `io.inputs` input callbacks hand their buffer to every
    /// worker, every output unit that is up runs `io.passes` times, and each
    /// worker pushes as `schedule` (one entry per runtime) says.
    fn cycle(&mut self, io: Io, schedule: &[Push]) {
        assert_eq!(schedule.len(), self.runtimes.len(), "one push per worker");
        for _ in 0..io.inputs {
            for queue in &mut self.pending {
                queue.push_back(self.input.clone());
            }
        }
        self.played.fill(0.0);
        for unit in &mut self.units {
            unit.heard = None;
        }
        for pass in 0..io.passes {
            for i in 0..self.units.len() {
                if pass == 0 {
                    for (rt, push) in schedule.iter().enumerate() {
                        if *push == Push::Midway(i) {
                            self.work(rt, 1);
                        }
                    }
                }
                if self.now < self.units[i].starts_at || self.stalled == Some(i) {
                    continue;
                }
                self.play_unit(i);
            }
        }
        for (rt, push) in schedule.iter().enumerate() {
            if *push != Push::Late {
                self.work(rt, 0);
            }
        }
        if self.gear_in_the_loop {
            for (input, played) in self
                .input
                .chunks_exact_mut(HD8_CHANNELS)
                .zip(self.played.chunks_exact(HD8_CHANNELS))
            {
                input[RETURN_L] = played[SEND];
                input[RETURN_R] = played[SEND];
            }
        }
        self.now += 1;
    }

    /// `cycles` normal cycles, every worker on time.
    fn run(&mut self, cycles: usize) {
        let on_time = self.all(Push::OnTime);
        for _ in 0..cycles {
            self.cycle(NORMAL, &on_time);
        }
    }

    /// A steady level on one input channel, every frame.
    fn hold(&mut self, channel: usize, level: f32) {
        for frame in self.input.chunks_exact_mut(HD8_CHANNELS) {
            frame[channel] = level;
        }
    }

    /// Runs `cycles` on time and returns, per channel of `channels`, the mean
    /// absolute level and the peak the HD 8 played there.
    fn listen(&mut self, cycles: usize, channels: &[usize]) -> (Vec<f32>, Vec<f32>) {
        let mut sum = vec![0.0_f64; channels.len()];
        let mut peak = vec![0.0_f32; channels.len()];
        self.listening = true;
        for _ in 0..cycles {
            self.run(1);
            for frame in self.played.chunks_exact(HD8_CHANNELS) {
                for (k, &ch) in channels.iter().enumerate() {
                    let sample = frame[ch].abs();
                    sum[k] += f64::from(sample);
                    peak[k] = peak[k].max(sample);
                }
            }
        }
        self.listening = false;
        let n = (cycles * FRAMES) as f64;
        (sum.into_iter().map(|s| (s / n) as f32).collect(), peak)
    }

    /// Only `sources` held at their levels: the mean level the HD 8 plays on
    /// each of `channels` once they have settled.
    fn levels(&mut self, sources: &[(usize, f32)], channels: &[usize]) -> Vec<f32> {
        self.input.fill(0.0);
        for &(ch, level) in sources {
            self.hold(ch, level);
        }
        self.run(LEVEL_SETTLE);
        self.listen(MEASURE, channels).0
    }

    /// After `idle` silent cycles, the frames from a pulse on every source to
    /// its first audible sample on each unit, every worker on time.
    fn pulse_latency(&mut self, idle: usize) -> Vec<Option<usize>> {
        let held = std::mem::replace(&mut self.input, silence());
        self.run(idle);
        let on_time = self.all(Push::OnTime);
        let mut latency: Vec<Option<usize>> = vec![None; self.units.len()];
        for cycle in 0..LISTEN {
            if cycle == 0 {
                for ch in SOURCES {
                    self.input[ch] = PULSE;
                }
            }
            self.cycle(NORMAL, &on_time);
            if cycle == 0 {
                for ch in SOURCES {
                    self.input[ch] = 0.0;
                }
            }
            for (slot, unit) in latency.iter_mut().zip(&self.units) {
                if slot.is_none() {
                    *slot = unit.heard.map(|frame| cycle * FRAMES + frame);
                }
            }
        }
        self.input = held;
        latency
    }

    /// Units that carry the same stream, keyed by the input channels of the
    /// pipelines writing them: Main and the second output of one head, or,
    /// with the insert on, every route the return writes.
    fn siblings(&self) -> Vec<(Vec<usize>, Vec<usize>)> {
        let mut by_producer: BTreeMap<Vec<usize>, Vec<usize>> = BTreeMap::new();
        for rt in &self.runtimes {
            for (input, routes) in pipelines(rt) {
                let group = by_producer.entry(input).or_default();
                for route in routes {
                    if let Some(i) = self.units.iter().position(|u| u.route == route) {
                        group.push(i);
                    }
                }
            }
        }
        by_producer
            .into_iter()
            .map(|(input, mut group)| {
                group.sort_unstable();
                group.dedup();
                (input, group)
            })
            .filter(|(_, group)| group.len() > 1)
            .collect()
    }

    /// (route, underrun frames, trims) each unit of `group` paid between two
    /// readings.
    fn paid(
        &self,
        before: &[(u64, u64)],
        after: &[(u64, u64)],
        group: &[usize],
    ) -> Vec<(usize, u64, u64)> {
        group
            .iter()
            .map(|&i| {
                (
                    self.units[i].route,
                    after[i].0 - before[i].0,
                    after[i].1 - before[i].1,
                )
            })
            .collect()
    }

    /// Runs `late` (the event, up to and including the cycle whose push
    /// catches up), then SETTLE cycles on time for the guard to shed what the
    /// catch-up left behind, then `watch` more. Returns what the event cost
    /// every unit.
    fn event(&mut self, late: impl FnOnce(&mut Rig), watch: usize) -> Vec<Outcome> {
        let start = self.counters();
        late(self);
        let back = self.counters();
        self.run(SETTLE);
        let settled = self.counters();
        let settled_fill = self.fills();
        self.run(watch);
        let end = self.counters();
        (0..self.units.len())
            .map(|i| Outcome {
                lost_while_late: back[i].0 - start[i].0,
                lost_after: end[i].0 - back[i].0,
                trims_to_settle: settled[i].1 - start[i].1,
                trims_after: end[i].1 - settled[i].1,
                settled_fill: settled_fill[i],
            })
            .collect()
    }

    /// The owner's invariants on one event, per unit: at most `late[w]`
    /// buffers of silence while its worker `w` was late, not one frame once
    /// the worker is back on time, at most `trims[w]` trims to shed what the
    /// catch-up left behind and none once settled, back at — never above —
    /// the fill it rested at before, and still in step with every route of
    /// its stream it was in step with.
    fn check(&self, what: &str, outcome: &[Outcome], late: &[u64], trims: &[u64], rest: &[usize]) {
        let frames = FRAMES as u64;
        for (i, o) in outcome.iter().enumerate() {
            let (worker, unit) = (self.units[i].runtime, self.name(i));
            assert!(
                o.lost_while_late <= late[worker] * frames,
                "{what}, {unit}: {} late buffer(s) of its worker cost {} underrun frames, \
                 more than the {} frames the worker failed to deliver",
                late[worker],
                o.lost_while_late,
                late[worker] * frames
            );
            assert_eq!(
                o.lost_after, 0,
                "{what}, {unit}: the worker was back on time, yet the route kept starving \
                 ({} more underrun frames) — the rig's underruns climbing in 64-frame \
                 multiples until a rebuild",
                o.lost_after
            );
            assert!(
                o.trims_to_settle <= trims[worker],
                "{what}, {unit}: {} trims to recover from {} late buffer(s); at most {} \
                 sheds what the catch-up really left behind",
                o.trims_to_settle,
                late[worker],
                trims[worker]
            );
            assert_eq!(
                o.trims_after, 0,
                "{what}, {unit}: {} trims after the route had settled with its worker on \
                 time — the guard cut a cushion that was not stuck (the rig: `latency_trims` \
                 rising until a rebuild)",
                o.trims_after
            );
            assert!(
                o.settled_fill <= rest[i],
                "{what}, {unit}: settled at {} queued frames, {} before the event — a late \
                 worker must never leave a route later than it was",
                o.settled_fill,
                rest[i]
            );
        }
        for i in 0..outcome.len() {
            for j in i + 1..outcome.len() {
                if self.units[i].runtime == self.units[j].runtime && rest[i] == rest[j] {
                    assert_eq!(
                        outcome[i].settled_fill,
                        outcome[j].settled_fill,
                        "{what}: {} and {} rested in step at {} frames; now {} vs {} — one \
                         stream a buffer apart on two outputs",
                        self.name(i),
                        self.name(j),
                        rest[i],
                        outcome[i].settled_fill,
                        outcome[j].settled_fill
                    );
                }
            }
        }
    }

    /// No unit plays a pulse later than it did before, and units of one
    /// stream that played it together still do.
    fn check_latency(&self, what: &str, before: &[Option<usize>], after: &[Option<usize>]) {
        for i in 0..self.units.len() {
            let (was, now) = match (before[i], after[i]) {
                (Some(was), Some(now)) => (was, now),
                _ => panic!(
                    "{what}, {}: the pulse was not heard (before {:?}, after {:?})",
                    self.name(i),
                    before[i],
                    after[i]
                ),
            };
            assert!(
                now <= was,
                "{what}, {}: a pulse now takes {now} frames, {was} before — route latency must \
                 never grow",
                self.name(i)
            );
            for j in i + 1..self.units.len() {
                if self.units[i].runtime == self.units[j].runtime && before[i] == before[j] {
                    assert_eq!(
                        after[i],
                        after[j],
                        "{what}: {} and {} played a pulse together before; now {:?} vs {:?} \
                         frames — the same note twice, a buffer apart",
                        self.name(i),
                        self.name(j),
                        after[i],
                        after[j]
                    );
                }
            }
        }
    }
}

/// (underrun-frame spread, trim spread) across a group of units.
fn spread(paid: &[(usize, u64, u64)]) -> (u64, u64) {
    let range = |values: Vec<u64>| -> u64 {
        values.iter().max().copied().unwrap_or(0) - values.iter().min().copied().unwrap_or(0)
    };
    (
        range(paid.iter().map(|p| p.1).collect()),
        range(paid.iter().map(|p| p.2).collect()),
    )
}

/// (route, channels, latency) of every unit, for a failure message.
fn labelled(rig: &Rig, latency: &[Option<usize>]) -> Vec<(usize, Vec<usize>, Option<usize>)> {
    rig.units
        .iter()
        .zip(latency)
        .map(|(u, l)| (u.route, u.channels.clone(), *l))
        .collect()
}

/// Every order in which `n` output units can start, one HAL cycle apart.
/// `order[route]` is the cycle of that route's first callback.
fn start_orders(n: usize) -> Vec<Vec<usize>> {
    let mut orders: Vec<Vec<usize>> = vec![Vec::new()];
    for next in 0..n {
        let mut grown = Vec::new();
        for order in &orders {
            for at in 0..=order.len() {
                let mut o = order.clone();
                o.insert(at, next);
                grown.push(o);
            }
        }
        orders = grown;
    }
    orders
}

/// xorshift64*: every run is the same run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) as usize % n
    }
}

/// One disturbance of the kinds a busy machine and a shared device produce,
/// picked at random, as in `issue_979_decay_and_device_stall_tests`: late
/// bursts, a catch-up between two output callbacks, one-buffer jitter,
/// lateness on the callback that closes a guard window, device-wide IO
/// stalls, lost and deferred input callbacks, skipped and repeated output
/// callbacks, and one output stream missing callbacks on its own.
fn disturb(rig: &mut Rig, rng: &mut Rng) -> &'static str {
    let workers = rig.runtimes.len();
    let units = rig.units.len();
    let on_time = rig.all(Push::OnTime);
    let late: Vec<Push> = if workers > 1 && rng.below(2) == 0 {
        let lone = rng.below(workers);
        (0..workers)
            .map(|w| if w == lone { Push::Late } else { Push::OnTime })
            .collect()
    } else {
        rig.all(Push::Late)
    };
    match rng.below(10) {
        0 => {
            for _ in 0..1 + rng.below(8) {
                rig.cycle(NORMAL, &late);
            }
            rig.cycle(NORMAL, &on_time);
            "late burst of 1 to 8"
        }
        1 => {
            rig.cycle(NORMAL, &late);
            let landing = rng.below(units + 1);
            let midway: Vec<Push> = late
                .iter()
                .map(|p| {
                    if *p == Push::Late {
                        Push::Midway(landing)
                    } else {
                        Push::OnTime
                    }
                })
                .collect();
            rig.cycle(NORMAL, &midway);
            "catch-up between two output callbacks"
        }
        2 => {
            let cycles = (1 + rng.below(4)) * WINDOW;
            let mut was_late = vec![false; workers];
            for _ in 0..cycles {
                let schedule: Vec<Push> = was_late
                    .iter_mut()
                    .map(|was| {
                        let push = if *was {
                            Push::Midway(rng.below(units + 1))
                        } else if rng.below(8) == 0 {
                            Push::Late
                        } else {
                            Push::OnTime
                        };
                        *was = push == Push::Late;
                        push
                    })
                    .collect();
                rig.cycle(NORMAL, &schedule);
            }
            rig.cycle(NORMAL, &on_time);
            "one-buffer jitter for 1 to 4 windows"
        }
        3 => {
            while rig.units[0].callbacks % WINDOW != WINDOW - 1 {
                rig.cycle(NORMAL, &on_time);
            }
            rig.cycle(NORMAL, &late);
            rig.cycle(NORMAL, &on_time);
            "late on the callback that closes a window"
        }
        4 => {
            rig.cycle(NORMAL, &late);
            for _ in 0..1 + rng.below(8) {
                rig.cycle(IO_STALLED, &on_time);
            }
            "every unit skipped 1 to 8 cycles"
        }
        5 => {
            for _ in 0..1 + rng.below(4) {
                rig.cycle(NO_INPUT, &on_time);
            }
            rig.cycle(NORMAL, &on_time);
            "input callback lost for 1 to 4 cycles"
        }
        6 => {
            let k = 1 + rng.below(4);
            for _ in 0..k {
                rig.cycle(NO_INPUT, &on_time);
            }
            rig.cycle(
                Io {
                    inputs: k + 1,
                    passes: 1,
                },
                &on_time,
            );
            "input deferred 1 to 4 cycles, then delivered back to back"
        }
        7 => {
            for _ in 0..1 + rng.below(4) {
                rig.cycle(OUTPUTS_SKIPPED, &on_time);
            }
            rig.cycle(NORMAL, &on_time);
            "every output unit skipped 1 to 4 cycles"
        }
        8 => {
            rig.cycle(OUTPUTS_SKIPPED, &on_time);
            rig.cycle(
                Io {
                    inputs: 1,
                    passes: 2,
                },
                &on_time,
            );
            "every output unit caught up with two callbacks"
        }
        _ => {
            rig.stalled = Some(rng.below(units));
            for _ in 0..1 + rng.below(3) {
                rig.cycle(NORMAL, &on_time);
            }
            rig.stalled = None;
            "one output stream missed 1 to 3 callbacks"
        }
    }
}

/// `HISTORY` (two simulated minutes): on-time stretches of 1 to 384 cycles (up to three
/// guard windows) between disturbances. Returns how many of each kind ran.
fn history(rig: &mut Rig, rng: &mut Rng) -> BTreeMap<&'static str, usize> {
    let end = rig.now + HISTORY;
    let mut tally = BTreeMap::new();
    while rig.now < end {
        let gap = 1 + rng.below(3 * WINDOW);
        rig.run(gap);
        *tally.entry(disturb(rig, rng)).or_insert(0) += 1;
    }
    tally
}

/// Late-worker recovery on the issue body's rig. One late buffer, at every
/// position of the guard's window: each route (`[24,25]` and `[10,11]`
/// included) loses at most that one buffer, nothing after the catch-up,
/// sheds the buffer the catch-up left behind at most once, and plays exactly
/// as late as before. On the symmetric rig route 0 `[0,1]` kept starving
/// (192 more underrun frames) after a late buffer at window phase 86.
#[test]
fn one_late_buffer_at_any_window_phase_costs_that_buffer_and_nothing_after() {
    // Late cycle + catch-up + SETTLE + watch = 7 windows + 1: every event
    // lands one callback later in the guard's window than the one before.
    const WATCH_ONE: usize = 4 * WINDOW - 1;
    for insert_enabled in [false, true] {
        let mut rig = Rig::warm(insert_enabled);
        let before = rig.pulse_latency(0);
        let rest = rig.fills();
        let workers = rig.runtimes.len();
        for phase in 0..WINDOW {
            let outcome = rig.event(
                |rig| {
                    let late = rig.all(Push::Late);
                    rig.cycle(NORMAL, &late);
                    rig.run(1);
                },
                WATCH_ONE,
            );
            rig.check(
                &format!(
                    "issue-body rig, insert {insert_enabled}, one late buffer at window phase \
                     {phase}"
                ),
                &outcome,
                &vec![1; workers],
                &vec![1; workers],
                &rest,
            );
        }
        let after = rig.pulse_latency(0);
        rig.check_latency(
            &format!("issue-body rig, insert {insert_enabled}, after {WINDOW} single late buffers"),
            &before,
            &after,
        );
    }
}

/// Late-worker recovery on the issue body's rig. Lateness at the SAME
/// position of the guard's window, three windows in a row, for every
/// position: a worker late on the callback that closes a window must not
/// teach a route a level it then keeps cutting to once the worker is on
/// time. On the symmetric rig route 0 `[0,1]` kept starving (128 more
/// underrun frames) after lateness at window phase 86.
#[test]
fn lateness_at_the_same_window_phase_every_window_stops_costing_when_it_stops() {
    const REPEATS: usize = 3;
    // REPEATS windows + SETTLE + watch = 8 windows + 1: each phase is one
    // callback later than the one before.
    const WATCH_PHASE: usize = 2 * WINDOW + 1;
    for insert_enabled in [false, true] {
        let mut rig = Rig::warm(insert_enabled);
        let before = rig.pulse_latency(0);
        let rest = rig.fills();
        let workers = rig.runtimes.len();
        for phase in 0..WINDOW {
            let outcome = rig.event(
                |rig| {
                    for _ in 0..REPEATS {
                        let late = rig.all(Push::Late);
                        rig.cycle(NORMAL, &late);
                        rig.run(WINDOW - 1);
                    }
                },
                WATCH_PHASE,
            );
            rig.check(
                &format!(
                    "issue-body rig, insert {insert_enabled}, late at window phase {phase} for \
                     {REPEATS} windows"
                ),
                &outcome,
                &vec![REPEATS as u64; workers],
                &vec![REPEATS as u64; workers],
                &rest,
            );
        }
        let after = rig.pulse_latency(0);
        rig.check_latency(
            &format!("issue-body rig, insert {insert_enabled}, after late windows at every phase"),
            &before,
            &after,
        );
    }
}

/// Decay spiral on the issue body's rig. Two simulated minutes of every
/// disturbance the rig can meet, then one minute with the worker and the
/// HD 8 on time: whatever the history taught the guards must be gone by then
/// without a rebuild — not one underrun and not one trim in the last 30 s on
/// any route, and every route resting exactly where a fresh build of the
/// chain rests. On the symmetric rig every route sat in a limit cycle of one
/// trim and one buffer of underrun every ~2 windows (5120 underrun frames and
/// 80 trims in the last 30 s on route 0), the owner's "loop" until off/on.
#[test]
fn damage_after_two_minutes_of_any_disturbance_decays_to_zero_without_a_rebuild() {
    for insert_enabled in [false, true] {
        let fresh_rest = Rig::warm(insert_enabled).fills();
        for seed in [0x0979_u64, 0x0979_5eed] {
            let mut rig = Rig::warm(insert_enabled);
            let mut rng = Rng((seed ^ (u64::from(insert_enabled) << 32)) | 1);
            let tally = history(&mut rig, &mut rng);
            let mut marks = vec![rig.counters()];
            for _ in 0..6 {
                rig.run(TEN_SECONDS);
                marks.push(rig.counters());
            }
            let failures: Vec<String> = (0..rig.units.len())
                .filter_map(|i| {
                    let lost = marks[6][i].0 - marks[3][i].0;
                    let trims = marks[6][i].1 - marks[3][i].1;
                    let fill = rig.units[i].fill;
                    if (lost, trims) == (0, 0) && fill == fresh_rest[i] {
                        return None;
                    }
                    let per_ten_seconds: Vec<(u64, u64)> = marks
                        .windows(2)
                        .map(|w| (w[1][i].0 - w[0][i].0, w[1][i].1 - w[0][i].1))
                        .collect();
                    Some(format!(
                        "  {}: {lost} underrun frames and {trims} trims in the last 30 s, \
                         resting at {fill} frames per callback start (a fresh build rests at \
                         {}); per 10 s of the on-time minute (underrun frames, trims): \
                         {per_ten_seconds:?}",
                        rig.name(i),
                        fresh_rest[i]
                    ))
                })
                .collect();
            assert!(
                failures.is_empty(),
                "issue-body rig, insert {insert_enabled}, seed {seed:#x}: after two minutes of \
                 disturbances ({tally:?}) and one minute with the worker and the HD 8 back on \
                 time, these routes still lose audio or kept latency — the rig's underruns and \
                 trims climbing until a rebuild:\n{}",
                failures.join("\n")
            );
        }
    }
}

/// Sibling divergence on the issue body's rig. Whatever order CoreAudio
/// started the output units in (every order, one cycle apart), every route
/// plays a pulse at the latency it has when every unit starts with the
/// input, and routes that carry the same stream (a head's Main and its own
/// `[24,25]` or `[10,11]`; with the insert on, every route the return
/// writes) play it together. On the symmetric rig a late-started unit kept
/// the buffers the input pushed before its first callback: siblings at 128
/// and 64 frames, the first capture's route 0 at fill 64 and route 2 at 128.
#[test]
fn every_route_plays_at_the_latency_it_has_when_its_stream_starts_with_the_input() {
    for insert_enabled in [false, true] {
        let mut together = Rig::starting(insert_enabled, &[]);
        let reference = together.pulse_latency(IDLE);
        assert!(
            reference.iter().all(Option::is_some),
            "precondition, issue-body rig, insert {insert_enabled}: every route plays the pulse \
             when every unit starts with the input: {:?}",
            labelled(&together, &reference)
        );
        for (producer, group) in together.siblings() {
            let heard: Vec<(usize, Option<usize>)> = group
                .iter()
                .map(|&i| (together.units[i].route, reference[i]))
                .collect();
            assert!(
                heard.iter().all(|(_, l)| *l == heard[0].1),
                "issue-body rig, insert {insert_enabled}, every unit started with the input: \
                 routes {heard:?} (route, frames) all play input {producer:?}, yet not together"
            );
        }
        for order in start_orders(together.units.len()) {
            let mut rig = Rig::starting(insert_enabled, &order);
            let latency = rig.pulse_latency(IDLE);
            assert!(
                latency == reference,
                "issue-body rig, insert {insert_enabled}, output units started at cycles \
                 {order:?} (by route): (route, channels, frames) {:?}; started with the input \
                 {:?}. A late unit kept the buffers the input pushed before its first callback \
                 (fills now {:?}) — latency grew, and siblings of one stream play a buffer apart",
                labelled(&rig, &latency),
                labelled(&together, &reference),
                rig.fills()
            );
        }
    }
}

/// Sibling divergence on the issue body's rig. One late worker push, swept
/// over every position of the guard's window, output units started one cycle
/// apart in route order: the routes of one stream saw the same push, so they
/// pay the same for it, within one buffer and one trim — Main and `[24,25]`
/// of `guitarra-1`, Main and `[10,11]` of `guitarra-2`, and with the insert
/// on every route the return writes. On the symmetric rig one sibling paid
/// 256 underrun frames and 3 trims, the other 64 and 1.
#[test]
fn one_late_push_costs_every_sibling_the_same_whatever_its_window_phase() {
    for insert_enabled in [false, true] {
        for phase in 0..WINDOW {
            let mut rig = Rig::starting(insert_enabled, &CAPTURE_ORDER);
            rig.warm_up();
            rig.run(phase);
            let before = rig.counters();
            let late = rig.all(Push::Late);
            rig.cycle(NORMAL, &late);
            rig.run(8 * WINDOW);
            let after = rig.counters();
            for (producer, group) in rig.siblings() {
                let paid = rig.paid(&before, &after, &group);
                let (underruns, trims) = spread(&paid);
                assert!(
                    underruns <= FRAMES as u64 && trims <= 1,
                    "issue-body rig, insert {insert_enabled}, phase {phase}: routes of input \
                     {producer:?} saw the same ONE late push, yet paid {paid:?} (route, \
                     underrun frames, trims) over the next ~1.5 s. One sibling kept paying \
                     after the worker was back on time (measured: 12032 vs 2496 underrun frames)"
                );
            }
        }
    }
}

/// Sibling divergence on the issue body's rig. Minutes of play on a busy
/// machine (one late push on every worker every ~1.4 s, 128 times), with the
/// insert off and on: the routes of one stream see the same load, so they end
/// within one buffer and one trim of each other. Measured on the rig: 12032
/// vs 2496 underrun frames on two routes of one stream; the symmetric harness
/// gave 77632 vs 51200.
#[test]
fn sibling_routes_pay_the_same_for_the_same_late_worker() {
    for insert_enabled in [false, true] {
        let mut rig = Rig::starting(insert_enabled, &CAPTURE_ORDER);
        rig.warm_up();
        let before = rig.counters();
        let late = rig.all(Push::Late);
        let on_time = rig.all(Push::OnTime);
        for cycle in 1..=LATE_EVENTS * LATE_EVERY {
            let schedule = if cycle % LATE_EVERY == 0 {
                &late
            } else {
                &on_time
            };
            rig.cycle(NORMAL, schedule);
        }
        rig.run(IDLE);
        let after = rig.counters();
        for (producer, group) in rig.siblings() {
            let paid = rig.paid(&before, &after, &group);
            let (underruns, trims) = spread(&paid);
            assert!(
                underruns <= FRAMES as u64 && trims <= 1,
                "issue-body rig, insert {insert_enabled}: routes of input {producer:?} saw the \
                 same {LATE_EVENTS} late pushes, yet paid {paid:?} (route, underrun frames, \
                 trims): {underruns} underrun frames and {trims} trims apart. Measured on the \
                 rig: 12032 vs 2496"
            );
        }
    }
}

/// Insert return stacking on the issue body's rig, insert on and off. Every
/// physical output plays each source exactly once, at the level a one-E/S
/// rig reaching that output plays it; the HD 8 sums the E/S, our code never
/// does, and per-stream volume is immutable.
///
/// Insert on, only the gear answering on the return (L 0.1, R 0.05): Main,
/// `[24,25]` and `[10,11]` each play the return once — neither a second copy
/// (Main is the one output both heads' routes share here; +6.02 dB on the
/// symmetric rig) nor a missing one (`[24,25]` and `[10,11]` are reached by
/// one head each, so dropping the return from one head's routes silences
/// them).
///
/// Insert off, both guitars (0.05 and 0.08) and a loud return (0.9, the SYN-2
/// still plugged in): Main plays each guitar once, `[24,25]` only
/// `guitarra-1`, `[10,11]` only `guitarra-2`, and the send and the tail carry
/// nothing of the switched-off loop.
#[test]
fn each_physical_output_plays_each_source_once_with_the_insert_on_or_off() {
    let mut wrong: Vec<String> = Vec::new();

    let the_return = [(RETURN_L, 0.1_f32), (RETURN_R, 0.05_f32)];
    let return_1 = Rig::one_e_s(true, "guitarra-1")
        .warmed()
        .levels(&the_return, &TAIL);
    let return_2 = Rig::one_e_s(true, "guitarra-2")
        .warmed()
        .levels(&the_return, &TAIL);
    let owner = Rig::new(true).warmed().levels(&the_return, &TAIL);
    for (k, &ch) in TAIL.iter().enumerate() {
        let (one, whose) = if OUT2.contains(&ch) {
            (return_2[k], "guitarra-2")
        } else {
            (return_1[k], "guitarra-1")
        };
        assert!(
            one > 1e-3,
            "precondition: the {whose}-only rig plays the return on out {ch} (level {one})"
        );
        let rise = db(owner[k] / one);
        if rise.abs() > SAME_DB {
            wrong.push(format!(
                "  insert on, gear only: out {ch} plays the return {rise:+.2} dB against the \
                 {whose}-only rig (issue-body rig {}, one E/S {one}) — {}",
                owner[k],
                if rise > 0.0 {
                    "a second copy stacked on it"
                } else {
                    "its copy is missing"
                }
            ));
        }
    }

    let (g1, g2) = (0.05_f32, 0.08_f32);
    let mut spots = TAIL.to_vec();
    spots.push(SEND);
    let alone_1 = Rig::one_e_s(false, "guitarra-1")
        .warmed()
        .levels(&[(GUITAR_1, g1)], &spots);
    let alone_2 = Rig::one_e_s(false, "guitarra-2")
        .warmed()
        .levels(&[(GUITAR_2, g2)], &spots);
    for (levels, whose, own) in [
        (&alone_1, "guitarra-1", [0usize, 2]),
        (&alone_2, "guitarra-2", [0usize, 4]),
    ] {
        for k in own {
            assert!(
                levels[k] > 1e-3,
                "precondition: the {whose}-only rig plays its guitar on out {} (level {})",
                spots[k],
                levels[k]
            );
        }
    }
    let owner = Rig::new(false).warmed().levels(
        &[
            (GUITAR_1, g1),
            (GUITAR_2, g2),
            (RETURN_L, 0.9),
            (RETURN_R, 0.9),
        ],
        &spots,
    );
    for (k, &ch) in spots.iter().enumerate() {
        let expected = alone_1[k] + alone_2[k];
        if !same_level(owner[k], expected) {
            wrong.push(format!(
                "  insert off, both guitars and a loud return: out {ch} plays {} where each \
                 guitar once gives {expected} (guitarra-1 alone {}, guitarra-2 alone {})",
                owner[k], alone_1[k], alone_2[k]
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "issue-body rig: a physical output does not play each source exactly once (the HD 8 \
         sums the E/S, our code never does; per-stream volume is immutable):\n{}",
        wrong.join("\n")
    );
}

/// Insert return stacking on the issue body's rig, end to end with the loop
/// closed through the gear at unity: both guitars (0.1 each) leave on the
/// send, come back on the return and reach the tail. Every physical output
/// peaks where a one-E/S rig reaching it peaks playing the same total (one
/// guitar at 0.2): Main as `guitarra-1` alone, `[24,25]` as `guitarra-1`
/// alone, `[10,11]` as `guitarra-2` alone. On the symmetric rig Main peaked
/// +6.02 dB (0.4 vs 0.2); stacked copies are what took the live output to
/// +10.1 dBFS.
#[test]
fn two_guitars_through_the_loop_never_peak_above_the_one_e_s_rigs() {
    fn through_the_loop(mut rig: Rig, guitars: &[(usize, f32)]) -> Vec<f32> {
        rig.gear_in_the_loop = true;
        rig.input.fill(0.0);
        for &(ch, level) in guitars {
            rig.hold(ch, level);
        }
        rig.run(WARM_UP);
        rig.listen(MEASURE, &TAIL).1
    }
    let alone_1 = through_the_loop(
        Rig::one_e_s(true, "guitarra-1").warmed(),
        &[(GUITAR_1, 0.2)],
    );
    let alone_2 = through_the_loop(
        Rig::one_e_s(true, "guitarra-2").warmed(),
        &[(GUITAR_2, 0.2)],
    );
    let owner = through_the_loop(Rig::new(true).warmed(), &[(GUITAR_1, 0.1), (GUITAR_2, 0.1)]);
    let mut wrong: Vec<String> = Vec::new();
    for (k, &ch) in TAIL.iter().enumerate() {
        let (one, whose) = if OUT2.contains(&ch) {
            (alone_2[k], "guitarra-2")
        } else {
            (alone_1[k], "guitarra-1")
        };
        assert!(
            one > 1e-3,
            "precondition: the {whose}-only rig hears the loop on out {ch} (peak {one})"
        );
        let rise = db(owner[k] / one);
        if rise.abs() > SAME_DB {
            wrong.push(format!(
                "  out {ch}: peaks {rise:+.2} dB against the {whose}-only rig playing the same \
                 total ({} vs {one})",
                owner[k]
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "issue-body rig, insert on, loop closed: the guitars or the return are stacked or \
         missing on a physical output:\n{}",
        wrong.join("\n")
    );
}
