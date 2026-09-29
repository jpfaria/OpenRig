//! #979 — hypothesis: what degrades the owner's rig is WHEN a switch lands in
//! the HAL cycle, not the switch itself.
//!
//! #967 (v0.5.1, the first release the owner hears the symptom on) swaps a
//! rebuilt runtime in within 5 ms (`rebuild_install_timer`), so a switch now
//! lands at any point of the cycle. Live rebuilds reuse the routes' rings
//! (#670, #969), so the switched chain inherits whatever the rings and the #670
//! worker held at that instant. The rig also rebuilds on its own (3 chain
//! rebuilds in 40 min with nobody touching it), so a switch can land while the
//! worker is behind (the dev build underruns from the first second with both
//! workers ~28% busy). The round-1 in-place tests only switched between cycles,
//! with the worker on time and every stream in lockstep. Three timings were
//! never tested:
//!
//! 1. The worker is 1 to 4 buffers behind when the switch lands. With the same
//!    streams, the backlog drains into the switched runtime. With new streams
//!    (the insert flip, which regroups this rig's two heads, #967), the old
//!    stream's worker still holds its backlog and the runtimes it was bound to.
//! 2. The swap lands after the input callback, between two output callbacks,
//!    or after them and before the worker push.
//! 3. The insert flip brings up new output streams 0 to 3 cycles after the new
//!    input, in any order, or 16 / 24 cycles after it (the head starts #965
//!    and #969 measured), on rings a live rebuild reuses. #969 measured that
//!    shape as `fill_frames: 1024`, the chain late and garbled until switched
//!    off and on.
//!
//! Owner invariants held here: a switch costs only the buffers the worker was
//! late with, and nothing once it is back on time; every frame plays once, and
//! every route has exactly one producer (N streams = N isolated pipelines);
//! routes that play one input stay aligned; latency never grows. After a
//! switch every route rests where a fresh build (the owner's off/on) rests,
//! within one guard window.
//!
//! The HAL cycle is the parent's (#965): the input callback, every started
//! output callback in route order, then the #670 worker. Each callback reads
//! its route's runtime from the runtime graph, as the stream seam reads its
//! live slot (#672). The SYN-2 return is driven as an input of its own: the
//! timing under test is the routes', not the gear's.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::f32::consts::TAU;
use std::ops::Range;
use std::sync::Arc;

use domain::ids::ChainId;
use project::chain::Chain;

use super::{
    gain, pipelines, rig_chain, rig_registry, written_routes, FRAMES, HD8_CHANNELS, RATE, ROUTES,
    TARGET,
};
use crate::runtime::{
    process_input_f32, process_output_f32, update_chain_runtime_state, RuntimeGraph,
};
use crate::runtime_state::ChainRuntimeState;

/// Callbacks per #953 guard window at the owner's 64-frame buffer: 8192
/// frames, ~186 ms (`docs/audio-config.md`).
const WINDOW: usize = 8_192 / FRAMES;
/// Cycles played before anything happens: past every cold start (~0.75 s).
const WARM_UP: usize = 4 * WINDOW;
/// "Within one guard window": the window a stream came up in, which the guard
/// cannot judge whole, plus one full window.
const WITHIN: usize = 2 * WINDOW;
/// What a route gets to shed what a catch-up left: the window the catch-up
/// landed in and three whole ones (~0.75 s).
const SETTLE: usize = 4 * WINDOW;
/// How long a recovered route must then stay whole (~1.5 s).
const WATCH: usize = 8 * WINDOW;
/// Cycles a pulse is listened for (~46 ms).
const LISTEN: usize = 32;
/// How far behind the worker is when the switch lands, in buffers.
const BACKLOGS: [usize; 4] = [1, 2, 3, 4];
/// The latest a new output stream comes up, in cycles after lockstep.
const MAX_LATE: usize = 3;
/// Input periods before the first output callback: #965's 16 and #969's 24.
const HEAD_STARTS: [usize; 2] = [16, 24];
/// In 1 (guitarra-1), In 2 (guitarra-2), In 3/4 (the SYN-2 return).
const SOURCES: [usize; 4] = [0, 1, 2, 3];
/// A 220 Hz guitar at −20 dBFS.
const LEVEL: f32 = 0.1;
const PULSE: f32 = 0.5;
/// Where the pulse sits in its input buffer.
const PULSE_FRAME: usize = FRAMES / 2;
/// A sample above this (−60 dBFS) is heard.
const HEARD: f32 = 1e-3;
/// A clean tone never steps more than this much further than a fresh build's
/// between two samples. A frame skipped, replayed or out of order steps ~50x.
const STEP_MARGIN: f32 = 1.5;
/// A doubled route is +6 dB.
const LEVEL_TOLERANCE_DB: f32 = 0.5;
/// Failures printed in full.
const SHOWN: usize = 16;

#[derive(Clone, Copy, Debug)]
struct Scene {
    insert: bool,
    preamp: bool,
    reverb: bool,
}

const fn scene(insert: bool, preamp: bool, reverb: bool) -> Scene {
    Scene {
        insert,
        preamp,
        reverb,
    }
}

/// The scene of the report: the SYN-2 in the loop.
const LOOP: Scene = scene(true, false, false);
const LOOP_FULL: Scene = scene(true, true, true);
/// `ANAL+DIG`, the scene of the live capture: the loop off, the preamp on.
const ANAL_DIG: Scene = scene(false, true, false);
const DRY_REVERB: Scene = scene(false, false, true);

/// Switches that keep the insert where it is, so the grouping and the streams
/// stay: the worker and its backlog survive them.
const IN_PLACE: [(Scene, Scene); 2] = [(LOOP, LOOP_FULL), (ANAL_DIG, DRY_REVERB)];
/// Insert flips: on this rig they regroup the heads (one runtime with the
/// loop on, one per guitar with it off), so the chain gets new streams.
const STRUCTURAL: [(Scene, Scene); 2] = [(LOOP, ANAL_DIG), (ANAL_DIG, LOOP)];

/// Per route: (underrun frames, latency trims).
type Damage = BTreeMap<usize, (u64, u64)>;
/// Per route: (lowest, highest) frames queued at the start of its callbacks.
type Rests = BTreeMap<usize, (usize, usize)>;
/// Per route: frames from the pulse entering the input to its first sample
/// heard on the route (negative: heard before it was played in).
type Latencies = BTreeMap<usize, Option<isize>>;

/// `rig:input-7` in `scene`: the parent's chain (insert, preamp, cab) plus a
/// VST3 reverb, all stood in by volume blocks.
fn chain_for(scene: Scene) -> Chain {
    let mut chain = rig_chain(scene.insert);
    chain.blocks.push(gain("reverb"));
    for block in chain.blocks.iter_mut() {
        match block.id.0.as_str() {
            "preamp" => block.enabled = scene.preamp,
            "reverb" => block.enabled = scene.reverb,
            _ => {}
        }
    }
    chain
}

fn chain_id() -> ChainId {
    rig_chain(false).id
}

/// The call the controller re-upserts a live chain through (#316).
fn upsert(graph: &mut RuntimeGraph, scene: Scene, needs_stream_rebuild: bool) {
    graph
        .upsert_chain(
            &chain_for(scene),
            RATE,
            &HashMap::new(),
            needs_stream_rebuild,
            &[TARGET; ROUTES],
            &rig_registry(),
        )
        .expect("the owner's chain must build");
}

/// The routes a scene owns: Main and Out 2 of each head, plus the send `[3]`
/// while the loop is on (pinned by the parent's layout tests).
fn routes_of(scene: Scene) -> Vec<usize> {
    (0..if scene.insert { ROUTES } else { ROUTES - 1 }).collect()
}

/// A steady 220 Hz guitar. It wraps every second, on a whole number of
/// periods, so it never steps.
fn tone(n: usize) -> f32 {
    LEVEL * (TAU * 220.0 * (n % 44_100) as f32 / RATE).sin()
}

fn queued(runtime: &ChainRuntimeState, route: usize) -> usize {
    runtime.output_routes.load()[route]
        .as_ref()
        .expect("a route the runtime writes")
        .buffer
        .len()
}

/// What each route paid between two readings.
fn since(before: &Damage, after: &Damage) -> Damage {
    after
        .iter()
        .map(|(route, (underruns, trims))| {
            let (u0, t0) = before.get(route).copied().unwrap_or((0, 0));
            (
                *route,
                (underruns.saturating_sub(u0), trims.saturating_sub(t0)),
            )
        })
        .collect()
}

/// Every order in which the output streams of `routes` routes come up, one
/// cycle apart, capped at `MAX_LATE`: route -> cycles after lockstep.
fn late_orders(routes: usize) -> Vec<BTreeMap<usize, usize>> {
    let mut orders: Vec<Vec<usize>> = vec![Vec::new()];
    for next in 0..routes {
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
    let distinct: BTreeSet<Vec<usize>> = orders
        .into_iter()
        .map(|order| order.into_iter().map(|late| late.min(MAX_LATE)).collect())
        .collect();
    distinct
        .into_iter()
        .map(|order| order.into_iter().enumerate().collect())
        .collect()
}

fn verdict(failures: &[String]) -> String {
    let shown: Vec<&str> = failures.iter().take(SHOWN).map(String::as_str).collect();
    let more = if failures.len() > SHOWN {
        format!("\n... and {} more", failures.len() - SHOWN)
    } else {
        String::new()
    };
    format!("{} failure(s):\n{}{more}", failures.len(), shown.join("\n"))
}

#[derive(Clone, Copy)]
enum Signal {
    Silence,
    Tone,
    /// One sample at `PULSE_FRAME` on every source.
    Pulse,
}

#[derive(Clone, Copy, Debug)]
enum Switch {
    /// A scene change that keeps the insert: the graph updates the live
    /// entries (#316); same streams.
    Graph,
    /// A chain holding a VST3: every live runtime updated in place (#779);
    /// same streams.
    InPlace,
    /// An insert flip: the heads regroup and the chain gets new streams (#967).
    Structural,
}

/// Where in one HAL cycle a swap lands.
#[derive(Clone, Copy, Debug)]
enum Landing {
    /// After the input callback, before any output callback.
    AfterInput,
    /// After the first `n` output callbacks (in route order) and before the
    /// rest.
    BetweenOutputs(usize),
    /// After every output callback, before the worker pushes this cycle's
    /// buffer.
    BeforePush,
    /// After the worker pushed: what the round-1 tests did.
    BetweenCycles,
}

fn landings(callbacks: usize) -> Vec<Landing> {
    let mut all = vec![Landing::AfterInput];
    all.extend((1..callbacks).map(Landing::BetweenOutputs));
    all.push(Landing::BeforePush);
    all.push(Landing::BetweenCycles);
    all
}

/// The owner's HD 8 with `rig:input-7` live in a runtime graph.
struct Rig {
    graph: RuntimeGraph,
    scene: Scene,
    /// Cycle of each live route's first output callback.
    starts: BTreeMap<usize, usize>,
    /// Input buffers the live #670 worker received and has not pushed yet,
    /// oldest first.
    pending: VecDeque<Vec<f32>>,
    /// The worker of a stream a structural switch replaced: the runtimes it
    /// was bound to and the buffers it still held.
    orphan: Option<(Vec<Arc<ChainRuntimeState>>, VecDeque<Vec<f32>>)>,
    input: Vec<f32>,
    out: Vec<f32>,
    now: usize,
    /// Frames of input generated so far.
    clock: usize,
    rests: Rests,
    /// Per route: (absolute frame of the first sample, what its first channel
    /// played), while recording.
    tapes: BTreeMap<usize, (usize, Vec<f32>)>,
    recording: bool,
}

impl Rig {
    /// The chain switched on: a fresh build. Its input stream's first callback
    /// is cycle 0; route `r`'s output stream comes up `1 + late[r]` (lockstep
    /// is 1: the cycle after the input's first, the healthy start of
    /// `cushion_shedding_969`).
    fn start(scene: Scene, late: &BTreeMap<usize, usize>) -> Self {
        let mut graph = RuntimeGraph {
            chains: HashMap::new(),
        };
        upsert(&mut graph, scene, true);
        Self {
            graph,
            scene,
            starts: routes_of(scene)
                .into_iter()
                .map(|route| (route, 1 + late.get(&route).copied().unwrap_or(0)))
                .collect(),
            pending: VecDeque::new(),
            orphan: None,
            input: vec![0.0; FRAMES * HD8_CHANNELS],
            out: vec![0.0; FRAMES * HD8_CHANNELS],
            now: 0,
            clock: 0,
            rests: Rests::new(),
            tapes: BTreeMap::new(),
            recording: false,
        }
    }

    /// Every runtime of the chain the graph holds, by group.
    fn live(&self) -> Vec<(usize, Arc<ChainRuntimeState>)> {
        let mut live = self.graph.runtimes_with_groups_for(&chain_id());
        live.sort_by_key(|(group, _)| *group);
        live
    }

    /// (runtime, route, first channel) of every output callback, in order.
    fn callbacks(&self) -> Vec<(Arc<ChainRuntimeState>, usize, usize)> {
        let mut list = Vec::new();
        for (_, runtime) in self.live() {
            for (route, channels) in written_routes(&runtime) {
                list.push((Arc::clone(&runtime), route, channels[0]));
            }
        }
        list
    }

    /// The input callback: the buffer goes to the worker's queue.
    fn begin(&mut self, signal: Signal) {
        let clock = self.clock;
        for (i, frame) in self.input.chunks_exact_mut(HD8_CHANNELS).enumerate() {
            let sample = match signal {
                Signal::Silence => 0.0,
                Signal::Tone => tone(clock + i),
                Signal::Pulse if i == PULSE_FRAME => PULSE,
                Signal::Pulse => 0.0,
            };
            for &ch in &SOURCES {
                frame[ch] = sample;
            }
        }
        self.clock += FRAMES;
        self.pending.push_back(self.input.clone());
    }

    /// The output callbacks `range` (in route order) of this cycle, every
    /// stream that is up.
    fn outputs(&mut self, range: Range<usize>) {
        let list = self.callbacks();
        let end = range.end.min(list.len());
        let start = range.start.min(end);
        for (runtime, route, channel) in &list[start..end] {
            if self.now < self.starts.get(route).copied().unwrap_or(0) {
                continue;
            }
            let fill = queued(runtime, *route);
            let rest = self.rests.entry(*route).or_insert((fill, fill));
            rest.0 = rest.0.min(fill);
            rest.1 = rest.1.max(fill);
            self.out.fill(0.0);
            process_output_f32(runtime, *route, &mut self.out, HD8_CHANNELS);
            if self.recording {
                let at = self.now * FRAMES;
                let (_, tape) = self.tapes.entry(*route).or_insert_with(|| (at, Vec::new()));
                tape.extend(self.out.chunks_exact(HD8_CHANNELS).map(|f| f[*channel]));
            }
        }
    }

    /// The #670 worker(s), after the output callbacks, unless late this
    /// cycle. A replaced stream's worker catches up on the runtimes it was
    /// bound to; the live worker pushes into the live ones.
    fn worker(&mut self, late: bool) {
        if late {
            return;
        }
        if let Some((bound, held)) = self.orphan.take() {
            for buffer in &held {
                for runtime in &bound {
                    process_input_f32(runtime, 0, buffer, HD8_CHANNELS);
                }
            }
        }
        let live = self.live();
        while let Some(buffer) = self.pending.pop_front() {
            for (_, runtime) in &live {
                process_input_f32(runtime, 0, &buffer, HD8_CHANNELS);
            }
        }
    }

    fn end(&mut self) {
        self.now += 1;
    }

    /// One HAL cycle. A late worker pushes nothing before the next cycle's
    /// output callbacks.
    fn cycle(&mut self, signal: Signal, late: bool) {
        self.begin(signal);
        self.outputs(0..usize::MAX);
        self.worker(late);
        self.end();
    }

    fn run(&mut self, signal: Signal, cycles: usize) {
        for _ in 0..cycles {
            self.cycle(signal, false);
        }
    }

    /// Switches to `next`. A structural switch brings up new streams: the new
    /// input's first callback is the next cycle, and route `r`'s output
    /// stream comes up `1 + late[r]` cycles after it. Returns every live
    /// route's counters right after the switch.
    fn switch(&mut self, kind: Switch, next: Scene, late: &BTreeMap<usize, usize>) -> Damage {
        match kind {
            Switch::Graph => {
                assert_eq!(
                    next.insert, self.scene.insert,
                    "setup: a same-stream switch keeps the insert, and so the grouping"
                );
                upsert(&mut self.graph, next, false);
            }
            Switch::InPlace => {
                assert_eq!(
                    next.insert, self.scene.insert,
                    "setup: an in-place switch keeps the insert, and so the grouping"
                );
                let chain = chain_for(next);
                for (_, runtime) in self.live() {
                    update_chain_runtime_state(
                        &runtime,
                        &chain,
                        RATE,
                        false,
                        &[TARGET; ROUTES],
                        &rig_registry(),
                    )
                    .expect("the in-place update must apply");
                }
            }
            Switch::Structural => {
                assert_ne!(
                    next.insert, self.scene.insert,
                    "setup: a structural switch flips the insert, which regroups the heads"
                );
                let bound: Vec<Arc<ChainRuntimeState>> = self
                    .live()
                    .into_iter()
                    .map(|(_, runtime)| runtime)
                    .collect();
                let held = std::mem::take(&mut self.pending);
                if !held.is_empty() {
                    self.orphan = Some((bound, held));
                }
                upsert(&mut self.graph, next, true);
                let first = self.now + 1;
                self.starts = routes_of(next)
                    .into_iter()
                    .map(|route| (route, first + late.get(&route).copied().unwrap_or(0)))
                    .collect();
            }
        }
        self.scene = next;
        self.damage()
    }

    /// One silent HAL cycle with a same-stream switch landing at `landing`.
    fn cycle_switching_at(&mut self, landing: Landing, kind: Switch, next: Scene) {
        let keep = BTreeMap::new();
        self.begin(Signal::Silence);
        let split = match landing {
            Landing::AfterInput => {
                self.switch(kind, next, &keep);
                0
            }
            Landing::BetweenOutputs(n) => n,
            Landing::BeforePush | Landing::BetweenCycles => usize::MAX,
        };
        self.outputs(0..split);
        if let Landing::BetweenOutputs(_) = landing {
            self.switch(kind, next, &keep);
        }
        self.outputs(split..usize::MAX);
        if let Landing::BeforePush = landing {
            self.switch(kind, next, &keep);
        }
        self.worker(false);
        self.end();
        if let Landing::BetweenCycles = landing {
            self.switch(kind, next, &keep);
        }
    }

    fn damage(&self) -> Damage {
        let mut damage = Damage::new();
        for (_, runtime) in self.live() {
            for (route, state) in runtime.output_routes.load().iter().enumerate() {
                if let Some(state) = state {
                    damage.insert(
                        route,
                        (state.buffer.underrun_count(), state.buffer.latency_trims()),
                    );
                }
            }
        }
        damage
    }

    /// Frames queued in every live route's ring right now.
    fn fills(&self) -> BTreeMap<usize, usize> {
        let mut fills = BTreeMap::new();
        for (_, runtime) in self.live() {
            for (route, state) in runtime.output_routes.load().iter().enumerate() {
                if let Some(state) = state {
                    fills.insert(route, state.buffer.len());
                }
            }
        }
        fills
    }

    fn take_rests(&mut self) -> Rests {
        std::mem::take(&mut self.rests)
    }

    fn record(&mut self) {
        self.tapes.clear();
        self.recording = true;
    }

    /// Per route: the absolute frame where each click was heard. Samples
    /// closer than a buffer to the one before belong to the same click.
    fn clicks(&self) -> BTreeMap<usize, Vec<usize>> {
        self.tapes
            .iter()
            .map(|(route, (from, tape))| {
                let mut events: Vec<usize> = Vec::new();
                let mut last: Option<usize> = None;
                for (i, sample) in tape.iter().enumerate() {
                    if sample.abs() <= HEARD {
                        continue;
                    }
                    let at = from + i;
                    match last {
                        Some(previous) if at - previous <= FRAMES => {}
                        _ => events.push(at),
                    }
                    last = Some(at);
                }
                (*route, events)
            })
            .collect()
    }

    /// Per route: (largest step between two samples, peak) of what it played
    /// while recording.
    fn played(&self) -> BTreeMap<usize, (f32, f32)> {
        self.tapes
            .iter()
            .map(|(route, (_, tape))| {
                let step = tape
                    .windows(2)
                    .map(|w| (w[1] - w[0]).abs())
                    .fold(0.0_f32, f32::max);
                let peak = tape.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
                (*route, (step, peak))
            })
            .collect()
    }

    /// A pulse on every source, then silence: when each route plays it.
    fn latencies(&mut self) -> Latencies {
        let pulse_at = (self.now * FRAMES + PULSE_FRAME) as isize;
        self.record();
        self.cycle(Signal::Pulse, false);
        self.run(Signal::Silence, LISTEN);
        self.recording = false;
        self.clicks()
            .into_iter()
            .map(|(route, events)| (route, events.first().map(|at| *at as isize - pulse_at)))
            .collect()
    }

    /// Routes that play the same input, keyed by its channels: Main and Out 2
    /// of one head, or, with the loop on, every route the return writes.
    fn siblings(&self) -> Vec<(Vec<usize>, Vec<usize>)> {
        let mut by_producer: BTreeMap<Vec<usize>, BTreeSet<usize>> = BTreeMap::new();
        for (_, runtime) in self.live() {
            for (input, routes) in pipelines(&runtime) {
                by_producer.entry(input).or_default().extend(routes);
            }
        }
        by_producer
            .into_iter()
            .filter(|(_, routes)| routes.len() > 1)
            .map(|(input, routes)| (input, routes.into_iter().collect()))
            .collect()
    }
}

/// What a burst of late worker buffers cost one rig.
struct Burst {
    /// Rest before the burst.
    rest_before: Rests,
    /// From the first late cycle until the route had settled.
    paid: Damage,
    /// Where the click in the first late buffer was heard, per route.
    clicks: BTreeMap<usize, Vec<usize>>,
    /// Rest once settled.
    rest_after: Rests,
    /// Once settled, over `WATCH`.
    paid_after: Damage,
}

/// The rig in `from`, warmed; then its worker misses `backlog` cycles in a
/// row (a click rides the first buffer it is late with); then, while it is
/// that far behind, `switch` lands (none for the control); then it catches up
/// and stays on time.
fn late_burst(from: Scene, backlog: usize, switch: Option<(Switch, Scene)>) -> Burst {
    let keep = BTreeMap::new();
    let mut rig = Rig::start(from, &keep);
    rig.run(Signal::Silence, WARM_UP);
    rig.take_rests();
    rig.run(Signal::Silence, WINDOW);
    let rest_before = rig.take_rests();
    let before = rig.damage();
    rig.record();
    for late in 0..backlog {
        let signal = if late == 0 {
            Signal::Pulse
        } else {
            Signal::Silence
        };
        rig.cycle(signal, true);
    }
    if let Some((kind, next)) = switch {
        rig.switch(kind, next, &keep);
    }
    rig.run(Signal::Silence, SETTLE);
    let paid = since(&before, &rig.damage());
    let clicks = rig.clicks();
    rig.recording = false;
    let settled = rig.damage();
    rig.take_rests();
    rig.run(Signal::Silence, WATCH);
    Burst {
        rest_before,
        paid,
        clicks,
        rest_after: rig.take_rests(),
        paid_after: since(&settled, &rig.damage()),
    }
}

/// Critic test 2. A scene switch that keeps the streams lands while the #670
/// worker is 1 to 4 buffers behind: the backlog it drains after the switch
/// goes through the switched runtime. The switch must change nothing about
/// what those late buffers cost: a rig that took the same late buffers and
/// did not switch is the reference, frame for frame. And the late buffers
/// themselves cost at most their own frames of silence and one shed of what
/// the catch-up left; each frame plays at most once; once the worker is back
/// on time the route pays nothing more and rests where it rested before.
/// Measured: continuous underruns in 64-frame multiples with trims until a
/// rebuild, and chain rebuilds landing with nobody touching the rig.
#[test]
fn a_switch_while_the_worker_is_behind_costs_only_the_late_buffers() {
    let mut by_switch = Vec::new();
    let mut by_lateness = Vec::new();
    for (from, to) in IN_PLACE {
        for backlog in BACKLOGS {
            let still = late_burst(from, backlog, None);
            for kind in [Switch::Graph, Switch::InPlace] {
                let what = format!(
                    "{kind:?} switch {from:?} -> {to:?} with the worker {backlog} buffer(s) behind"
                );
                let s = late_burst(from, backlog, Some((kind, to)));
                if s.paid != still.paid
                    || s.clicks != still.clicks
                    || s.rest_after != still.rest_after
                {
                    by_switch.push(format!(
                        "{what}: (underrun frames, trims) by route {:?}, the same late buffers \
                         with no switch {:?}; the late click heard at frames {:?} vs {:?}; rest \
                         once settled {:?} vs {:?}",
                        s.paid, still.paid, s.clicks, still.clicks, s.rest_after, still.rest_after
                    ));
                }
                for (route, events) in &s.clicks {
                    if events.len() > 1 {
                        by_switch.push(format!(
                            "{what}: route {route} played the one late click {} times (frames \
                             {events:?}): a buffer played twice",
                            events.len()
                        ));
                    }
                }
                let late_frames = (backlog * FRAMES) as u64;
                for (route, (underruns, trims)) in &s.paid {
                    if *underruns > late_frames || *trims > 1 {
                        by_lateness.push(format!(
                            "{what}: route {route} paid {underruns} underrun frames and {trims} \
                             trims for {late_frames} late frames (at most {late_frames} and one \
                             shed)"
                        ));
                    }
                }
                for (route, damage) in &s.paid_after {
                    if *damage != (0, 0) {
                        by_lateness.push(format!(
                            "{what}: route {route} kept paying {damage:?} (underrun frames, \
                             trims) over ~1.5 s with the worker back on time"
                        ));
                    }
                }
                if s.rest_after != s.rest_before {
                    by_lateness.push(format!(
                        "{what}: the routes rest at {:?} frames per callback start, {:?} before \
                         the late buffers: the event left latency behind",
                        s.rest_after, s.rest_before
                    ));
                }
            }
        }
    }
    assert!(
        by_switch.is_empty() && by_lateness.is_empty(),
        "caused by the switch (compared with the same late buffers and no switch): {}\n\n\
         what the late buffers cost the switched rig: {}",
        verdict(&by_switch),
        verdict(&by_lateness)
    );
}

/// Critic test 2, new streams. The insert flip (a regroup on this rig) lands
/// while the worker is 1 to 4 buffers behind, a click in the first buffer it
/// is late with. Those buffers belong to the old input stream: its worker
/// still holds them and the runtimes it was bound to, and catches up after
/// the switch. They are the late buffers the switch may cost, and nothing
/// else: no route of the new build ever plays them (one producer per route,
/// N streams = N isolated pipelines), and the new build pays, rests and plays
/// exactly what a fresh build of the same scene (the owner's off/on) does,
/// within one guard window.
#[test]
fn a_structural_switch_while_the_worker_is_behind_costs_only_the_late_buffers() {
    let keep = BTreeMap::new();
    let mut failures = Vec::new();
    for (from, to) in STRUCTURAL {
        let mut fresh = Rig::start(to, &keep);
        fresh.run(Signal::Silence, 1 + WITHIN);
        let fresh_paid = fresh.damage();
        fresh.take_rests();
        fresh.run(Signal::Silence, WINDOW);
        let fresh_rest = fresh.take_rests();
        for backlog in BACKLOGS {
            let what = format!(
                "insert flip {from:?} -> {to:?} with the worker {backlog} buffer(s) behind"
            );
            let mut rig = Rig::start(from, &keep);
            rig.run(Signal::Silence, WARM_UP);
            for late in 0..backlog {
                let signal = if late == 0 {
                    Signal::Pulse
                } else {
                    Signal::Silence
                };
                rig.cycle(signal, true);
            }
            let at_switch = rig.switch(Switch::Structural, to, &keep);
            rig.record();
            rig.run(Signal::Silence, 1 + WITHIN);
            let paid = since(&at_switch, &rig.damage());
            let judged_from = rig.damage();
            rig.take_rests();
            rig.run(Signal::Silence, WINDOW);
            let rest = rig.take_rests();
            let judged = since(&judged_from, &rig.damage());
            for (route, events) in rig.clicks() {
                if !events.is_empty() {
                    failures.push(format!(
                        "{what}: route {route} of the new build played the old stream's late \
                         click at frames {events:?}: the old worker's backlog reached a live \
                         ring, two producers on one route"
                    ));
                }
            }
            for (route, (underruns, trims)) in &paid {
                let (fresh_underruns, fresh_trims) =
                    fresh_paid.get(route).copied().unwrap_or((0, 0));
                if *underruns > fresh_underruns || *trims > fresh_trims {
                    failures.push(format!(
                        "{what}: route {route} paid {underruns} underrun frames and {trims} \
                         trims in its first ~0.4 s; a fresh build paid {fresh_underruns} and \
                         {fresh_trims}"
                    ));
                }
            }
            if rest != fresh_rest {
                failures.push(format!(
                    "{what}: one guard window on, the routes rest at {rest:?} frames per \
                     callback start; a fresh build rests at {fresh_rest:?}"
                ));
            }
            for (route, damage) in &judged {
                if *damage != (0, 0) {
                    failures.push(format!(
                        "{what}: route {route} still paid {damage:?} (underrun frames, trims) \
                         a guard window after the switch"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", verdict(&failures));
}

/// Critic test 3. #967's install timer lands a switch at any point of the HAL
/// cycle: after the input callback, between two output callbacks (one
/// sibling read before it, the other after), or after the outputs and before
/// the worker pushes the buffer the input handed it. Wherever it lands, a
/// same-stream switch leaves every ring as it was: the routes rest, and pay,
/// exactly what a fresh build of the new scene does, and a pulse played right
/// after takes the same number of frames on every sibling route as on that
/// fresh build. A sibling a buffer apart is the "several streams stacked";
/// the first capture had route 0 at fill 64 and route 2 at 128.
#[test]
fn a_switch_landing_between_the_input_and_the_worker_push_leaves_siblings_aligned() {
    let keep = BTreeMap::new();
    let mut failures = Vec::new();
    for (from, to) in IN_PLACE {
        let mut fresh = Rig::start(to, &keep);
        fresh.run(Signal::Silence, WARM_UP);
        fresh.take_rests();
        fresh.run(Signal::Silence, WINDOW);
        let fresh_rest = fresh.take_rests();
        let fresh_latency = fresh.latencies();
        let callbacks = fresh.callbacks().len();
        for kind in [Switch::Graph, Switch::InPlace] {
            for landing in landings(callbacks) {
                let what = format!("{kind:?} switch {from:?} -> {to:?} landing {landing:?}");
                let mut rig = Rig::start(from, &keep);
                rig.run(Signal::Silence, WARM_UP);
                let before = rig.damage();
                rig.take_rests();
                rig.cycle_switching_at(landing, kind, to);
                rig.run(Signal::Silence, WINDOW - 1);
                let rest = rig.take_rests();
                let latency = rig.latencies();
                let paid = since(&before, &rig.damage());
                for (producer, routes) in rig.siblings() {
                    let heard: Vec<(usize, Option<isize>)> = routes
                        .iter()
                        .map(|route| (*route, latency.get(route).copied().flatten()))
                        .collect();
                    if heard.iter().any(|(_, l)| l.is_none() || *l != heard[0].1) {
                        failures.push(format!(
                            "{what}: routes {routes:?} all play input {producer:?}, yet play a \
                             pulse after {heard:?} frames (fills now {:?})",
                            rig.fills()
                        ));
                    }
                }
                if latency != fresh_latency {
                    failures.push(format!(
                        "{what}: a pulse takes {latency:?} frames by route; on a fresh build \
                         of the scene, {fresh_latency:?}"
                    ));
                }
                if rest != fresh_rest {
                    failures.push(format!(
                        "{what}: over the window after the switch the routes rest at {rest:?} \
                         frames per callback start; a fresh build rests at {fresh_rest:?}"
                    ));
                }
                for (route, damage) in &paid {
                    if *damage != (0, 0) {
                        failures.push(format!(
                            "{what}: route {route} paid {damage:?} (underrun frames, trims) for \
                             a switch with the worker on time"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", verdict(&failures));
}

/// Critic test 4. The insert flip regroups the heads and brings up new
/// streams: the input first, then each output stream 0 to 3 cycles after
/// lockstep, in every order, on whatever rings the rebuild reuses. Latency
/// never grows and never depends on when a stream came up: within one guard
/// window every route rests where a fresh build (off/on, every stream in
/// lockstep) rests, it never underran on the way there, it shed what the late
/// start left at most once, and it pays nothing after. (A fresh build whose
/// streams come up late has the same fault, pinned by
/// `cushion_shedding_969::a_route_whose_output_started_late_rests_with_its_siblings`;
/// the next test isolates what the switch adds.)
#[test]
fn a_structural_switch_whose_new_streams_start_late_rests_like_a_fresh_build() {
    let keep = BTreeMap::new();
    let judge_at = 1 + MAX_LATE + WITHIN;
    let mut failures = Vec::new();
    for (from, to) in STRUCTURAL {
        let mut fresh = Rig::start(to, &keep);
        fresh.run(Signal::Tone, judge_at);
        fresh.take_rests();
        fresh.run(Signal::Tone, WINDOW);
        let reference = fresh.take_rests();
        for late in late_orders(routes_of(to).len()) {
            let what = format!(
                "insert flip {from:?} -> {to:?}, new output streams up {late:?} cycles after \
                 lockstep (route: cycles)"
            );
            let mut rig = Rig::start(from, &keep);
            rig.run(Signal::Tone, WARM_UP);
            let at_switch = rig.switch(Switch::Structural, to, &late);
            rig.run(Signal::Tone, judge_at);
            let paid = since(&at_switch, &rig.damage());
            let judged_from = rig.damage();
            rig.take_rests();
            rig.run(Signal::Tone, WINDOW);
            let rests = rig.take_rests();
            let judged = since(&judged_from, &rig.damage());
            if rests != reference {
                failures.push(format!(
                    "{what}: one guard window on, the routes rest at {rests:?} frames per \
                     callback start; a fresh build rests at {reference:?} (fills now {:?})",
                    rig.fills()
                ));
            }
            for (route, (underruns, trims)) in &paid {
                if *underruns > 0 || *trims > 1 {
                    failures.push(format!(
                        "{what}: route {route} paid {underruns} underrun frames and {trims} \
                         trims coming up (at most one shed, never an underrun)"
                    ));
                }
            }
            for (route, damage) in &judged {
                if *damage != (0, 0) {
                    failures.push(format!(
                        "{what}: route {route} still paid {damage:?} (underrun frames, trims) \
                         a guard window after its stream came up"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", verdict(&failures));
}

/// The switch-only part of critic test 4. A switch is off/on done fast: after
/// the insert flip, whatever order the new output streams came up in, every
/// route plays a pulse exactly as late as it does after the owner's off/on
/// with the streams coming up in the same order. Anything more is what the
/// switch carried over (a reused ring's fill, a guard's learned level): the
/// #969 "late until switched off and on".
#[test]
fn a_structural_switch_plays_every_route_as_late_as_off_and_on_with_the_same_start_order() {
    let keep = BTreeMap::new();
    let mut failures = Vec::new();
    for (from, to) in STRUCTURAL {
        for late in late_orders(routes_of(to).len()) {
            let mut off_on = Rig::start(to, &late);
            off_on.run(Signal::Silence, 1 + MAX_LATE + SETTLE);
            let reference = off_on.latencies();
            let mut rig = Rig::start(from, &keep);
            rig.run(Signal::Tone, WARM_UP);
            rig.switch(Switch::Structural, to, &late);
            rig.run(Signal::Silence, 1 + MAX_LATE + SETTLE);
            let heard = rig.latencies();
            if heard != reference {
                failures.push(format!(
                    "insert flip {from:?} -> {to:?}, new output streams up {late:?} cycles \
                     after lockstep (route: cycles): a pulse takes {heard:?} frames by route; \
                     after off/on with the same start order, {reference:?} (fills now {:?} vs \
                     {:?})",
                    rig.fills(),
                    off_on.fills()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", verdict(&failures));
}

/// Critic scenario 5, #969 on the two-head rig. The insert flip brings up the
/// new output streams 16 or 24 input periods after the new input (#965's and
/// #969's measured head starts), on rings the rebuild reuses: #969 read
/// `fill_frames: 1024`, the chain late and garbled until switched off and on.
/// Every route sheds that head start at most once and never underruns; one
/// guard window on it rests where off/on with the same head start rests and
/// pays nothing more; and it plays the guitar clean: never a step a fresh
/// build's tone does not take (a frame skipped, replayed or out of order),
/// never louder or softer than a fresh build (a stacked copy is +6 dB).
#[test]
fn the_969_head_start_after_a_structural_switch_is_shed_once_and_never_garbles() {
    let keep = BTreeMap::new();
    let mut failures = Vec::new();
    for (from, to) in STRUCTURAL {
        let mut lockstep = Rig::start(to, &keep);
        lockstep.run(Signal::Tone, 1 + WITHIN);
        lockstep.record();
        lockstep.run(Signal::Tone, WINDOW);
        let clean = lockstep.played();
        for (route, (_, peak)) in &clean {
            assert!(
                *peak > HEARD,
                "setup: a fresh build of {to:?} must play the tone on route {route} (peak {peak})"
            );
        }
        for head in HEAD_STARTS {
            let what = format!(
                "insert flip {from:?} -> {to:?}, new output streams up {head} periods after \
                 the new input"
            );
            let late: BTreeMap<usize, usize> = routes_of(to)
                .into_iter()
                .map(|route| (route, head - 1))
                .collect();
            let mut off_on = Rig::start(to, &late);
            off_on.run(Signal::Tone, head + WITHIN);
            off_on.take_rests();
            off_on.run(Signal::Tone, WINDOW);
            let reference = off_on.take_rests();
            let mut rig = Rig::start(from, &keep);
            rig.run(Signal::Tone, WARM_UP);
            let at_switch = rig.switch(Switch::Structural, to, &late);
            rig.run(Signal::Tone, head + WITHIN);
            let paid = since(&at_switch, &rig.damage());
            let judged_from = rig.damage();
            rig.take_rests();
            rig.record();
            rig.run(Signal::Tone, WINDOW);
            let rests = rig.take_rests();
            let judged = since(&judged_from, &rig.damage());
            let played = rig.played();
            for (route, (underruns, trims)) in &paid {
                if *underruns > 0 || *trims > 1 {
                    failures.push(format!(
                        "{what}: route {route} paid {underruns} underrun frames and {trims} \
                         trims shedding the head start (once, never an underrun)"
                    ));
                }
            }
            if rests != reference {
                failures.push(format!(
                    "{what}: one guard window on, the routes rest at {rests:?} frames per \
                     callback start; after off/on with the same head start, {reference:?} \
                     (fills now {:?})",
                    rig.fills()
                ));
            }
            for (route, damage) in &judged {
                if *damage != (0, 0) {
                    failures.push(format!(
                        "{what}: route {route} still paid {damage:?} (underrun frames, trims) \
                         a guard window after its stream came up"
                    ));
                }
            }
            for (route, (clean_step, clean_peak)) in &clean {
                let Some((step, peak)) = played.get(route).copied() else {
                    failures.push(format!("{what}: route {route} played nothing"));
                    continue;
                };
                let db = 20.0 * (peak.max(HEARD) / clean_peak).log10();
                if step > clean_step * STEP_MARGIN + 1e-6 || db.abs() > LEVEL_TOLERANCE_DB {
                    failures.push(format!(
                        "{what}: route {route} steps up to {step:.4} between two samples (a fresh \
                         build {clean_step:.4}) and peaks {peak:.4} ({db:+.1} dB against a fresh \
                         build): the tone is garbled"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", verdict(&failures));
}
