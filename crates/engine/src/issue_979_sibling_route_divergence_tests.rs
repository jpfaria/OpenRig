//! #979 — routes that carry the same stream, and the two heads that share the
//! same physical channels, must degrade together or not at all.
//!
//! Measured on the owner's rig: two routes of the same stream reached 12032
//! and 2496 underrun frames (188 and 39 whole 64-frame buffers) under the same
//! load. In the live capture the two heads' groups stayed within one buffer of
//! each other (256 and 320).
//!
//! Hypothesis: the damage a late DSP worker does to a route is decided by
//! route-local state that has nothing to do with the load. That state is set
//! only by when the route's CoreAudio unit happened to start:
//!
//! 1. Its rest fill. The input stream starts first, and every input period
//!    before an output's first callback pushes a buffer nobody pops (#965). A
//!    unit that starts two or more cycles after the input keeps a full ring
//!    (128 frames at the start of its callback) for the life of the chain. A
//!    unit that starts with the input rests at 64. The two siblings then play
//!    one buffer (1.45 ms) apart, which matches the first capture: route 0 at
//!    fill 64, route 2 at 128, both on Main.
//! 2. The phase of its drift-guard window (~186 ms, 128 callbacks at 64
//!    frames). A late push leaves the ring empty on ONE callback. When that is
//!    the callback that closes a route's window, the route learns a level of 0
//!    (`elastic_drift_guard_tests::an_empty_ring_at_the_closing_callback_is_not_a_level`).
//!    From then on it cuts its cushion and underruns at window rate until a
//!    rebuild. Its sibling's window closes on another callback, so the same
//!    push costs the sibling one buffer. On this hypothesis, the 39 buffers
//!    are the late pushes both routes saw, and the other 149 are the poisoned
//!    route's own cuts.
//!
//! These tests hold the rig to the owner's invariants. Routes that carry the
//! same stream play it at one latency. Under the same load they pay the same
//! (within one buffer). Nothing that one head's worker or one route's stream
//! does reaches the other head or a sibling (N streams = N isolated
//! pipelines). Latency never grows. No route plays two copies of its stream.
//!
//! The HAL cycle is the parent's, as #965 measured it: the input callback,
//! then every started output callback, then each runtime's #670 worker. This
//! file adds three things: each output stream starts on its own cycle, a
//! stream can miss callbacks, and each runtime's worker can be late on its own.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use super::{pipelines, runtimes, written_routes, FRAMES, HD8_CHANNELS, ROUTES};
use crate::runtime::{process_input_f32, process_output_f32};
use crate::runtime_state::ChainRuntimeState;

/// In 1 (guitarra-1), In 2 (guitarra-2), In 3/4 (the SYN-2 return).
const SOURCES: [usize; 4] = [0, 1, 2, 3];
/// A steady guitar, the level the parent's HAL plays.
const LEVEL: f32 = 0.1;
const PULSE: f32 = 0.5;
const HEARD: f32 = 1e-3;
/// Output streams starting one HAL cycle apart, in route order. Under the
/// hypothesis, this order gives the first capture's fills (route 0 at 64,
/// route 2 at 128).
const CAPTURE_ORDER: [usize; ROUTES] = [0, 1, 2, 3, 4];
const WARM_UP: usize = 1_000;
/// Idle cycles before a pulse (8 guard windows, ~1.5 s). This is long enough
/// for every route to shed what the guard is meant to shed.
const SETTLE: usize = 1_024;
const LISTEN: usize = 32;
/// Callbacks per drift-guard window at 64 frames (~186 ms).
const WINDOW: usize = 128;
/// One late worker push every ~1.4 s (a busy machine), as in the parent. It
/// is coprime with the window, so the pushes land on every window position.
const LATE_EVERY: usize = 997;
const LATE_EVENTS: usize = 256;

/// One CoreAudio output unit of the HD 8, i.e. one route of the chain.
struct Stream {
    runtime: usize,
    route: usize,
    /// First channel the route writes, where its signal is read back.
    channel: usize,
    /// Cycle of the unit's first callback, counted from the input's first.
    starts_at: usize,
}

struct Rig {
    runtimes: Vec<Arc<ChainRuntimeState>>,
    streams: Vec<Stream>,
    input: Vec<f32>,
    out: Vec<f32>,
    /// Per runtime: input buffers its worker has not pushed yet.
    pending: Vec<VecDeque<Vec<f32>>>,
    now: usize,
}

impl Rig {
    /// `starts_at[route]` is the cycle of that route's first output callback
    /// (a missing entry means it starts with the input).
    fn new(insert_enabled: bool, starts_at: &[usize]) -> Self {
        let runtimes = runtimes(insert_enabled);
        let mut streams = Vec::new();
        for (runtime, rt) in runtimes.iter().enumerate() {
            for (route, channels) in written_routes(rt) {
                streams.push(Stream {
                    runtime,
                    route,
                    channel: channels[0],
                    starts_at: starts_at.get(route).copied().unwrap_or(0),
                });
            }
        }
        let pending = vec![VecDeque::new(); runtimes.len()];
        Self {
            runtimes,
            streams,
            input: vec![0.0; FRAMES * HD8_CHANNELS],
            out: vec![0.0; FRAMES * HD8_CHANNELS],
            pending,
            now: 0,
        }
    }

    /// One HAL cycle. `late` lists the runtimes whose worker misses this
    /// cycle: its push lands after the NEXT cycle's output callbacks.
    /// `stalled` lists the routes whose output callback does not run this
    /// cycle. Returns, for every route served, the first frame of this cycle
    /// that carries signal.
    fn cycle(
        &mut self,
        level: f32,
        pulse: bool,
        late: &[usize],
        stalled: &[usize],
    ) -> Vec<(usize, Option<usize>)> {
        self.input.fill(0.0);
        for frame in self.input.chunks_exact_mut(HD8_CHANNELS) {
            for &ch in &SOURCES {
                frame[ch] = level;
            }
        }
        if pulse {
            for &ch in &SOURCES {
                self.input[ch] = PULSE;
            }
        }
        // The input callback: the buffer is queued for every runtime's worker.
        for queue in &mut self.pending {
            queue.push_back(self.input.clone());
        }
        // This cycle's output callbacks, one per started, running unit.
        let mut heard = Vec::new();
        for s in &self.streams {
            if self.now < s.starts_at || stalled.contains(&s.route) {
                continue;
            }
            self.out.fill(0.0);
            process_output_f32(
                &self.runtimes[s.runtime],
                s.route,
                &mut self.out,
                HD8_CHANNELS,
            );
            let first = self
                .out
                .chunks_exact(HD8_CHANNELS)
                .position(|f| f[s.channel].abs() > HEARD);
            heard.push((s.route, first));
        }
        // Each worker pushes after them, unless it is late this cycle.
        for (i, rt) in self.runtimes.iter().enumerate() {
            if late.contains(&i) {
                continue;
            }
            while let Some(buffer) = self.pending[i].pop_front() {
                process_input_f32(rt, 0, &buffer, HD8_CHANNELS);
            }
        }
        self.now += 1;
        heard
    }

    /// Plays until every unit has started and is past its first windows.
    fn warm_up(&mut self) {
        let last_start = self.streams.iter().map(|s| s.starts_at).max().unwrap_or(0);
        for _ in 0..last_start + WARM_UP {
            self.cycle(LEVEL, false, &[], &[]);
        }
    }

    /// Plays `events` late pushes on the workers of `late`, one every
    /// `LATE_EVERY` cycles, then lets every route settle on time.
    fn late_worker(&mut self, events: usize, late: &[usize]) {
        for cycle in 1..=events * LATE_EVERY {
            let late_now: &[usize] = if cycle % LATE_EVERY == 0 { late } else { &[] };
            self.cycle(LEVEL, false, late_now, &[]);
        }
        for _ in 0..SETTLE {
            self.cycle(LEVEL, false, &[], &[]);
        }
    }

    /// After a silent settle, the frames from a pulse on every source to its
    /// first sample on each route, by route.
    fn latencies(&mut self) -> BTreeMap<usize, usize> {
        for _ in 0..SETTLE {
            self.cycle(0.0, false, &[], &[]);
        }
        let mut latency = BTreeMap::new();
        for p in 0..LISTEN {
            for (route, first) in self.cycle(0.0, p == 0, &[], &[]) {
                if let Some(frame) = first {
                    latency.entry(route).or_insert(p * FRAMES + frame);
                }
            }
        }
        latency
    }

    /// (underrun frames, latency trims) of every route, by route.
    fn damage(&self) -> BTreeMap<usize, (u64, u64)> {
        let mut damage = BTreeMap::new();
        for rt in &self.runtimes {
            let routes = rt.output_routes.load();
            for (route, r) in routes.iter().enumerate() {
                if let Some(r) = r.as_ref() {
                    damage.insert(route, (r.buffer.underrun_count(), r.buffer.latency_trims()));
                }
            }
        }
        damage
    }

    /// Frames queued in every route's ring right now, by route.
    fn fills(&self) -> BTreeMap<usize, usize> {
        let mut fills = BTreeMap::new();
        for rt in &self.runtimes {
            let routes = rt.output_routes.load();
            for (route, r) in routes.iter().enumerate() {
                if let Some(r) = r.as_ref() {
                    fills.insert(route, r.buffer.len());
                }
            }
        }
        fills
    }

    /// Each route's peak since the last read, in dBFS, by route.
    fn peaks(&self) -> BTreeMap<usize, f64> {
        let mut peaks = BTreeMap::new();
        for rt in &self.runtimes {
            for row in rt.take_output_route_stats() {
                peaks.insert(row.route, f64::from(row.peak_dbfs));
            }
        }
        peaks
    }
}

/// Routes that carry the same stream, keyed by the input channels of the
/// pipelines writing them. That is Main and Out 2 of one head, or, with the
/// insert on, every route the return writes.
fn siblings(rig: &Rig) -> Vec<(Vec<usize>, Vec<usize>)> {
    let mut by_producer: BTreeMap<Vec<usize>, Vec<usize>> = BTreeMap::new();
    for rt in &rig.runtimes {
        for (input, routes) in pipelines(rt) {
            by_producer.entry(input).or_default().extend(routes);
        }
    }
    by_producer
        .into_iter()
        .map(|(input, mut routes)| {
            routes.sort_unstable();
            routes.dedup();
            (input, routes)
        })
        .filter(|(_, routes)| routes.len() > 1)
        .collect()
}

/// Routes of every runtime that write the same physical channels, keyed by
/// those channels. With the insert off, this is the two heads' Main and the
/// two heads' Out 2.
fn same_channels(rig: &Rig) -> Vec<(Vec<usize>, Vec<usize>)> {
    let mut by_channels: BTreeMap<Vec<usize>, Vec<usize>> = BTreeMap::new();
    for rt in &rig.runtimes {
        for (route, channels) in written_routes(rt) {
            by_channels.entry(channels).or_default().push(route);
        }
    }
    by_channels
        .into_iter()
        .filter(|(_, routes)| routes.len() > 1)
        .collect()
}

/// (route, underrun frames, trims) each route paid between two readings.
fn paid_by(
    before: &BTreeMap<usize, (u64, u64)>,
    after: &BTreeMap<usize, (u64, u64)>,
    routes: &[usize],
) -> Vec<(usize, u64, u64)> {
    routes
        .iter()
        .map(|&route| {
            let (u0, t0) = before[&route];
            let (u1, t1) = after[&route];
            (route, u1 - u0, t1 - t0)
        })
        .collect()
}

/// (underrun-frame spread, trim spread) across a group of routes.
fn spread(paid: &[(usize, u64, u64)]) -> (u64, u64) {
    let range = |values: Vec<u64>| -> u64 {
        values.iter().max().copied().unwrap_or(0) - values.iter().min().copied().unwrap_or(0)
    };
    (
        range(paid.iter().map(|p| p.1).collect()),
        range(paid.iter().map(|p| p.2).collect()),
    )
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

/// Routes that play the same input must play it at one latency, whatever
/// order CoreAudio started their units in. Those routes are Main and Out 2 of
/// one head, or, with the insert on, every route the return writes (two of
/// which are Main). #953 already requires this: the same note leaving two
/// outputs apart is the "stacked" sound. Checked for every start order, one
/// cycle apart. The first capture read route 0 at fill 64 and route 2 at 128.
#[test]
fn sibling_routes_rest_at_the_same_latency_whatever_order_their_streams_started() {
    for insert_enabled in [false, true] {
        let units = Rig::new(insert_enabled, &[]).streams.len();
        for order in start_orders(units) {
            let mut rig = Rig::new(insert_enabled, &order);
            let latency = rig.latencies();
            let fills = rig.fills();
            for (producer, routes) in siblings(&rig) {
                let heard: Vec<(usize, Option<usize>)> = routes
                    .iter()
                    .map(|r| (*r, latency.get(r).copied()))
                    .collect();
                assert!(
                    heard.iter().all(|(_, l)| l.is_some() && *l == heard[0].1),
                    "insert {insert_enabled}, output units started at cycles {order:?} \
                     (by route): routes {routes:?} all play input {producer:?}, yet they \
                     play its pulse after {heard:?} frames (fills now {fills:?}). The \
                     same note leaves those outputs a buffer apart. The first capture \
                     read route 0 at fill 64 and route 2 at 128"
                );
            }
        }
    }
}

/// A route's rest level must not depend on when its unit started. Whatever
/// the rest is, every route has the latency it has when every unit starts
/// with the input. A unit that started k cycles late must not keep those k
/// buffers (up to its ring) as latency for the life of the chain. Latency
/// never grows.
#[test]
fn a_route_latency_does_not_depend_on_when_its_stream_started() {
    for insert_enabled in [false, true] {
        let mut together = Rig::new(insert_enabled, &[]);
        let reference = together.latencies();
        assert_eq!(
            reference.len(),
            together.streams.len(),
            "insert {insert_enabled}: every route plays the pulse when all units \
             start with the input: {reference:?}"
        );
        for order in start_orders(together.streams.len()) {
            let mut rig = Rig::new(insert_enabled, &order);
            let latency = rig.latencies();
            assert_eq!(
                latency,
                reference,
                "insert {insert_enabled}, output units started at cycles {order:?} (by \
                 route): the latencies (by route, frames) moved from those of units \
                 started with the input. A late unit kept the buffers the input pushed \
                 before its first callback (fills now {:?})",
                rig.fills()
            );
        }
    }
}

/// One late worker push, swept over every position of the guard's window.
/// Main and Out 2 of one head saw the same push, so they pay the same for it,
/// within one buffer and one trim. Under the hypothesis, the push that empties
/// the ring on one route's closing callback poisons that route alone. It then
/// keeps underrunning for the 8 windows that follow while its sibling paid
/// once. That is the measured 12032 vs 2496, starting from a single push.
#[test]
fn one_late_push_costs_every_sibling_the_same_whatever_its_window_phase() {
    for phase in 0..WINDOW {
        let mut rig = Rig::new(false, &CAPTURE_ORDER);
        rig.warm_up();
        for _ in 0..phase {
            rig.cycle(LEVEL, false, &[], &[]);
        }
        let before = rig.damage();
        let every_worker: Vec<usize> = (0..rig.runtimes.len()).collect();
        rig.cycle(LEVEL, false, &every_worker, &[]);
        for _ in 0..8 * WINDOW {
            rig.cycle(LEVEL, false, &[], &[]);
        }
        let after = rig.damage();
        for (producer, routes) in siblings(&rig) {
            let paid = paid_by(&before, &after, &routes);
            let (underruns, trims) = spread(&paid);
            assert!(
                underruns <= FRAMES as u64 && trims <= 1,
                "phase {phase}: routes {routes:?} all play input {producer:?} and saw \
                 the same ONE late push, yet paid {paid:?} (route, underrun frames, \
                 trims) over the next ~1.5 s. One sibling kept paying after the worker \
                 was back on time (measured: 12032 vs 2496 underrun frames)"
            );
        }
    }
}

/// Minutes of play on a busy machine, with the insert off and on. Every
/// worker is late at the same cycles, so every route of a stream sees the
/// same load. Siblings must end within one buffer and one trim of each other.
/// Measured on the rig: 12032 vs 2496 underrun frames on two routes of one
/// stream.
#[test]
fn sibling_routes_pay_the_same_for_the_same_late_worker() {
    for insert_enabled in [false, true] {
        let mut rig = Rig::new(insert_enabled, &CAPTURE_ORDER);
        rig.warm_up();
        let before = rig.damage();
        let every_worker: Vec<usize> = (0..rig.runtimes.len()).collect();
        rig.late_worker(LATE_EVENTS, &every_worker);
        let after = rig.damage();
        for (producer, routes) in siblings(&rig) {
            let paid = paid_by(&before, &after, &routes);
            let (underruns, trims) = spread(&paid);
            assert!(
                underruns <= FRAMES as u64 && trims <= 1,
                "insert {insert_enabled}: routes {routes:?} all play input {producer:?} \
                 and saw the same {LATE_EVENTS} late pushes, yet paid {paid:?} (route, \
                 underrun frames, trims): {underruns} underrun frames and {trims} trims \
                 apart. Measured on the rig: 12032 vs 2496"
            );
        }
    }
}

/// With the insert off, the two heads share the same physical channels:
/// routes 0 and 2 on Main, routes 1 and 3 on Out 2. They are two runtimes
/// with two workers. The same CPU contention makes both workers late at the
/// same cycles, so each pair must pay the same, within one buffer and one
/// trim. The live capture had them 256 vs 320: per-route state must not turn
/// the same load into divergent damage.
#[test]
fn the_two_heads_on_the_same_channels_pay_the_same_for_the_same_load() {
    let mut rig = Rig::new(false, &CAPTURE_ORDER);
    rig.warm_up();
    let before = rig.damage();
    rig.late_worker(LATE_EVENTS / 2, &[0, 1]);
    let after = rig.damage();
    let groups = same_channels(&rig);
    assert_eq!(
        groups.len(),
        2,
        "the two heads share Main and Out 2: {groups:?}"
    );
    for (channels, routes) in groups {
        let paid = paid_by(&before, &after, &routes);
        let (underruns, trims) = spread(&paid);
        assert!(
            underruns <= FRAMES as u64 && trims <= 1,
            "routes {routes:?} both write {channels:?}. Both heads' workers were late \
             at the same {} cycles, yet the routes paid {paid:?} (route, underrun \
             frames, trims). The live capture had the two heads 256 vs 320",
            LATE_EVENTS / 2
        );
    }
}

/// Isolation: one head's worker running late never reaches the other head.
/// The on-time head's routes take no underrun and no trim, and play at the
/// latency they had before, even though they share Main and Out 2 with the
/// late head.
#[test]
fn a_late_worker_on_one_head_never_reaches_the_other_head() {
    for late_head in [0usize, 1] {
        let on_time = 1 - late_head;
        let mut rig = Rig::new(false, &CAPTURE_ORDER);
        let routes: Vec<usize> = rig
            .streams
            .iter()
            .filter(|s| s.runtime == on_time)
            .map(|s| s.route)
            .collect();
        let latency_before = rig.latencies();
        let before = rig.damage();
        rig.late_worker(LATE_EVENTS / 2, &[late_head]);
        let latency_after = rig.latencies();
        let after = rig.damage();
        for route in &routes {
            assert_eq!(
                after[route], before[route],
                "route {route} of head {on_time}: its worker was always on time, yet it \
                 paid (underrun frames, trims) for head {late_head}'s late worker"
            );
            assert_eq!(
                latency_after.get(route),
                latency_before.get(route),
                "route {route} of head {on_time}: its latency moved because head \
                 {late_head}'s worker was late"
            );
        }
    }
}

/// Isolation between siblings: Out 2 of guitarra-1 misses three callbacks
/// (a stalled output unit). Every other route, whether its sibling on the
/// same stream or the other head's, takes no underrun and no trim, and keeps
/// its latency. The stalled route itself comes back to its siblings' latency
/// (#953, on this rig's 64-frame buffer).
#[test]
fn an_output_stall_on_one_route_leaves_its_siblings_and_the_other_head_untouched() {
    const STALLED: usize = 1;
    const MISSED: usize = 3;
    for insert_enabled in [false, true] {
        let mut rig = Rig::new(insert_enabled, &[]);
        let latency_before = rig.latencies();
        let before = rig.damage();
        for _ in 0..MISSED {
            rig.cycle(LEVEL, false, &[], &[STALLED]);
        }
        let latency_after = rig.latencies();
        let after = rig.damage();
        for (route, damage) in &before {
            if *route == STALLED {
                continue;
            }
            assert_eq!(
                after[route], *damage,
                "insert {insert_enabled}: route {route} paid (underrun frames, trims) \
                 for route {STALLED}'s stalled unit"
            );
        }
        assert_eq!(
            latency_after, latency_before,
            "insert {insert_enabled}: route {STALLED}'s unit missed {MISSED} callbacks \
             and the rig then idled ~1.5 s, yet the latencies (by route, frames) moved. \
             A stall leaves no latency on the stalled route or on anyone else"
        );
    }
}

/// A route carries one copy of its own stream. Trims and underruns on it or on
/// its siblings never stack a second copy on top. A route's peak while the
/// worker runs late stays at its steady peak. The output reached +10.1 dBFS on
/// the rig.
#[test]
fn no_route_ever_plays_more_than_one_copy_of_its_stream() {
    for insert_enabled in [false, true] {
        let mut rig = Rig::new(insert_enabled, &CAPTURE_ORDER);
        rig.warm_up();
        rig.peaks();
        for _ in 0..WARM_UP {
            rig.cycle(LEVEL, false, &[], &[]);
        }
        let steady = rig.peaks();
        let every_worker: Vec<usize> = (0..rig.runtimes.len()).collect();
        rig.late_worker(LATE_EVENTS / 2, &every_worker);
        let degraded = rig.peaks();
        for (route, steady_db) in &steady {
            let late_db = degraded[route];
            assert!(
                late_db <= steady_db + 0.1,
                "insert {insert_enabled}, route {route}: steady peak {steady_db:.1} dBFS, \
                 {late_db:.1} dBFS while the worker ran late. Something stacked a second \
                 copy on the route (the rig reached +10.1 dBFS)"
            );
        }
    }
}
