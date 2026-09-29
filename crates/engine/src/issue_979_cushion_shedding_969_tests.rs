//! #979 — cushion shedding (#969) and the #953 drift guard on the owner's
//! two-head `rig:input-7` (child of `issue_979_two_head_rig_tests`).
//!
//! Hypothesis: the #969 cap — "a route's level is at most its elastic target
//! plus the period its callback pops" — was written for a producer that pushes
//! BEFORE the output callback of its cycle. On this rig the #670 worker pushes
//! AFTER every output callback of the cycle (#965's HAL measurement), so a
//! route whose output stream came up after the input had already pushed two
//! buffers rests one whole buffer (64 frames, 1.45 ms) above a sibling that
//! came up in lockstep, and the guard ratifies it as "the cushion the route
//! was built for" — the first live capture's route 0 at fill 64 next to route
//! 2 at 128. The routes of one stream then sit at different depths and in
//! different guard phases, so the same late worker buffer lands differently on
//! each: a route whose window closes on the empty ring learns a level of 0 and
//! cuts its cushion every window from then on (underruns climbing in 64-frame
//! multiples until a rebuild), while its sibling does not (12032 vs 2496
//! underruns, measured on the same stream).
//!
//! Owner invariants pinned here on the real topology: a healthy route is never
//! cut; a cut never produces an underrun on the callbacks after it; the level a
//! route learns never leaves it empty at a callback start; latency never grows
//! and never depends on when a route's output stream started; one guitar's
//! late worker never touches the other guitar's routes.
//!
//! The HAL cycle is the parent's: the input callback, then the callback of
//! every output stream that is up, then each runtime's #670 worker pushes what
//! it received. A worker held back pushes after the NEXT cycle's outputs.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::{runtimes, written_routes, FRAMES, HD8_CHANNELS, ROUTES};
use crate::runtime::{process_input_f32, process_output_f32};
use crate::runtime_state::ChainRuntimeState;

/// The #953 guard judges a route once per ~186 ms window (8192 frames,
/// `docs/audio-config.md`): 128 callbacks at the owner's 64-frame buffer.
const WINDOW: usize = 8_192 / FRAMES;
/// One late worker buffer every three windows plus one callback: each late
/// buffer lands one callback further into the guard's window than the one
/// before, so `LATE_BUFFERS` of them hit every position of it — the callback
/// that closes it included — and each has three windows to be absorbed and
/// shed before the next.
const LATE_EVERY: usize = 3 * WINDOW + 1;
const LATE_BUFFERS: usize = WINDOW;
/// cpal starts the chain's input stream, then its output streams one after
/// another ("1 input(s), 5 output(s)"): route `r` delivers its first callback
/// `STAGGERED[r]` cycles after the input's first.
const STAGGERED: [usize; ROUTES] = [1, 2, 3, 4, 5];
/// Every output stream up in the cycle right after the input's first: no
/// route holds more than the one buffer the worker handed off.
const LOCKSTEP: [usize; ROUTES] = [1; ROUTES];
/// Out 2 `[10,11]` of `guitarra-1` in both scenes; its sibling Main is route 0.
const OUT_2_OF_GUITAR_1: usize = 1;

/// (lowest, highest) frames queued at the start of a route's callbacks.
type Rest = (usize, usize);

fn on_time(_cycle: usize, _runtime: usize) -> bool {
    false
}

/// Highest minus lowest of a per-route count.
fn spread(counts: &[u64]) -> u64 {
    counts.iter().max().copied().unwrap_or(0) - counts.iter().min().copied().unwrap_or(0)
}

/// Frames queued in a route's ring right now.
fn queued(runtime: &ChainRuntimeState, route: usize) -> usize {
    runtime.output_routes.load()[route]
        .as_ref()
        .expect("a route the runtime writes")
        .buffer
        .len()
}

struct Rig {
    runtimes: Vec<Arc<ChainRuntimeState>>,
    /// Routes each runtime writes. Each runtime has its own #670 worker.
    routes: Vec<Vec<usize>>,
    /// Cycle of each route's first output callback.
    starts: [usize; ROUTES],
    /// A route whose output stream misses its callbacks while set.
    stalled: Option<usize>,
    input: Vec<f32>,
    out: Vec<f32>,
    /// Per runtime: input buffers its worker received but has not pushed.
    backlog: Vec<usize>,
    cycle: usize,
}

impl Rig {
    fn new(insert_enabled: bool, starts: [usize; ROUTES]) -> Self {
        let runtimes = runtimes(insert_enabled);
        let routes: Vec<Vec<usize>> = runtimes
            .iter()
            .map(|rt| {
                written_routes(rt)
                    .into_iter()
                    .map(|(route, _)| route)
                    .collect()
            })
            .collect();
        let mut input = vec![0.0; FRAMES * HD8_CHANNELS];
        for frame in input.chunks_mut(HD8_CHANNELS) {
            frame[0] = 0.1;
            frame[1] = 0.1;
        }
        Self {
            backlog: vec![0; runtimes.len()],
            runtimes,
            routes,
            starts,
            stalled: None,
            input,
            out: vec![0.0; FRAMES * HD8_CHANNELS],
            cycle: 0,
        }
    }

    /// `cycles` HAL cycles. `late(cycle, runtime)` holds that runtime's worker
    /// back this cycle. Returns the rest every route that ran showed.
    fn run(&mut self, cycles: usize, late: impl Fn(usize, usize) -> bool) -> BTreeMap<usize, Rest> {
        let mut rests: BTreeMap<usize, Rest> = BTreeMap::new();
        for _ in 0..cycles {
            // Input callback: every worker receives the buffer.
            for pending in self.backlog.iter_mut() {
                *pending += 1;
            }
            // Output callbacks of this cycle, every stream that is up.
            for (owner, runtime) in self.runtimes.iter().enumerate() {
                for &route in &self.routes[owner] {
                    if self.cycle < self.starts[route] || self.stalled == Some(route) {
                        continue;
                    }
                    let fill = queued(runtime, route);
                    let rest = rests.entry(route).or_insert((fill, fill));
                    rest.0 = rest.0.min(fill);
                    rest.1 = rest.1.max(fill);
                    self.out.fill(0.0);
                    process_output_f32(runtime, route, &mut self.out, HD8_CHANNELS);
                }
            }
            // Each runtime's worker, after the output callbacks.
            for (owner, runtime) in self.runtimes.iter().enumerate() {
                if late(self.cycle, owner) {
                    continue;
                }
                for _ in 0..self.backlog[owner] {
                    process_input_f32(runtime, 0, &self.input, HD8_CHANNELS);
                }
                self.backlog[owner] = 0;
            }
            self.cycle += 1;
        }
        rests
    }

    /// (underrun frames, latency trims) of every route, by route.
    fn damage(&self) -> BTreeMap<usize, (u64, u64)> {
        self.runtimes
            .iter()
            .flat_map(|rt| {
                rt.output_routes
                    .load()
                    .iter()
                    .enumerate()
                    .filter_map(|(route, r)| {
                        r.as_ref()
                            .map(|r| (route, (r.buffer.underrun_count(), r.buffer.latency_trims())))
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn damage_since(&self, before: &BTreeMap<usize, (u64, u64)>) -> BTreeMap<usize, (u64, u64)> {
        self.damage()
            .into_iter()
            .map(|(route, (underruns, trims))| {
                let (u0, t0) = before[&route];
                (route, (underruns - u0, trims - t0))
            })
            .collect()
    }
}

/// Control. Every stream up in lockstep and the worker always on time: over
/// ~7 s every route of both scenes — the send `[3]`, which both guitars feed,
/// included — holds one steady rest, the same on every route and never empty
/// at a callback start, and is never cut nor starved.
#[test]
fn a_healthy_two_head_rig_is_never_cut() {
    for insert_enabled in [false, true] {
        let mut rig = Rig::new(insert_enabled, LOCKSTEP);
        let rests = rig.run(40 * WINDOW, on_time);
        // Insert on: the return's Main and Out 2 (once per physical output,
        // #979) and the send; insert off: each guitar's Main and Out 2.
        assert_eq!(
            rests.len(),
            if insert_enabled { 3 } else { 4 },
            "insert {insert_enabled}: every route of the scene ran: {rests:?}"
        );
        let first = *rests.values().next().expect("the rig has routes");
        for (route, rest) in &rests {
            assert!(
                rest.0 == rest.1 && *rest == first && rest.0 >= FRAMES,
                "insert {insert_enabled}: route {route} rested at {rest:?} frames at its \
                 callback starts (every route: {rests:?}) — a healthy route holds one steady \
                 rest, the same as its siblings, and is never empty"
            );
        }
        for (route, (underruns, trims)) in rig.damage() {
            assert_eq!(
                (underruns, trims),
                (0, 0),
                "insert {insert_enabled}: healthy route {route} was starved {underruns} \
                 frames and cut {trims} times with the worker always on time"
            );
        }
    }
}

/// #969's shape at start-up. cpal brings the output streams up one after
/// another, and every input period before a route's first callback leaves a
/// buffer in its ring: a route whose stream came up two cycles late is full
/// (128 frames, its whole ring) at its first callback. The guard takes target
/// + one period as the cushion the route was built for and never sheds it, so
/// that route plays one buffer (1.45 ms) behind its sibling for the life of
/// the chain — the first live capture had route 0 at fill 64 and route 2 at
/// 128. Latency must not depend on when a route's stream started: after ~7 s
/// every route rests where the same route of a lockstep start does.
#[test]
fn a_route_whose_output_started_late_rests_with_its_siblings() {
    for insert_enabled in [false, true] {
        let mut lockstep = Rig::new(insert_enabled, LOCKSTEP);
        lockstep.run(40 * WINDOW, on_time);
        let expected = lockstep.run(WINDOW, on_time);
        let mut staggered = Rig::new(insert_enabled, STAGGERED);
        staggered.run(40 * WINDOW, on_time);
        let rests = staggered.run(WINDOW, on_time);
        for (route, expected_rest) in &expected {
            let rest = rests[route];
            assert_eq!(
                rest,
                *expected_rest,
                "insert {insert_enabled}: route {route}, whose output stream came up {} cycles \
                 after the input, still rests at {rest:?} frames per callback start after ~7 s; \
                 the same route started in lockstep rests at {expected_rest:?} — it plays {} \
                 frames behind the rest of its stream for good (all routes: {rests:?})",
                STAGGERED[*route],
                rest.0 as i64 - expected_rest.0 as i64
            );
        }
    }
}

/// The #969 shed must never starve the route it sheds. Every stream comes up
/// after the input ran ahead by #965's 16 and #969's 24 periods, so every ring
/// is full at its first callback. Whatever the guard sheds, it sheds once, no
/// route is ever empty at a callback start and none underruns.
#[test]
fn shedding_a_head_start_never_starves_the_route() {
    for head_start in [16usize, 24] {
        for insert_enabled in [false, true] {
            let mut rig = Rig::new(insert_enabled, [head_start; ROUTES]);
            let rests = rig.run(head_start + 40 * WINDOW, on_time);
            for (route, (underruns, trims)) in rig.damage() {
                assert!(
                    underruns == 0 && trims <= 1,
                    "insert {insert_enabled}, head start {head_start}: route {route} was cut \
                     {trims} times and starved {underruns} frames — shedding what the head \
                     start left must happen once and never cost an underrun"
                );
            }
            for (route, rest) in rests {
                assert!(
                    rest.0 >= FRAMES,
                    "insert {insert_enabled}, head start {head_start}: route {route} was \
                     left with {} frames at a callback start after the shed (rest {rest:?}) \
                     — less than the one buffer the callback pops",
                    rest.0
                );
            }
        }
    }
}

/// The level a route learns must never leave it empty at a callback start. A
/// late worker buffer is one buffer of silence and one shed wherever it lands
/// in the guard's window — the callback that closes it included, where the
/// ring is empty but that callback's own underrun is not counted yet.
/// `LATE_BUFFERS` late buffers hit every position of the window; once the
/// worker is on time for good every route must be clean again, back at the
/// rest it held before. The live symptom is the opposite: underruns climbing
/// in 64-frame multiples and trims rising until the chain is rebuilt (the
/// sibling rig test measured 60096 underrun frames and 758 trims).
#[test]
fn once_the_worker_is_back_on_time_every_route_is_clean_again() {
    for insert_enabled in [false, true] {
        let mut rig = Rig::new(insert_enabled, LOCKSTEP);
        rig.run(4 * WINDOW, on_time);
        let before = rig.run(WINDOW, on_time);
        let t0 = rig.cycle;
        rig.run(LATE_EVERY * LATE_BUFFERS, move |cycle, _| {
            (cycle - t0) % LATE_EVERY == LATE_EVERY - 1
        });
        // The last late buffer's own window, then the shed of what it left.
        rig.run(3 * WINDOW, on_time);
        let settled = rig.damage();
        let after = rig.run(20 * WINDOW, on_time);
        for (route, (underruns, trims)) in rig.damage_since(&settled) {
            assert_eq!(
                (underruns, trims),
                (0, 0),
                "insert {insert_enabled}: route {route} kept losing audio with the worker \
                 back on time: {underruns} underrun frames and {trims} trims in ~3.7 s — a \
                 late buffer taught its guard a level below one buffer"
            );
        }
        assert_eq!(
            after, before,
            "insert {insert_enabled}: the routes did not return to the rest they held before \
             the late buffers (before {before:?}, after {after:?})"
        );
    }
}

/// N streams = N pipelines. With the insert off each guitar is its own
/// runtime with its own worker; `guitarra-1`'s worker late at every position
/// of the guard's window must not cost `guitarra-2`'s Main and Out 2 a single
/// underrun, a trim or a frame of rest.
#[test]
fn one_guitars_late_worker_never_reaches_the_other_guitars_routes() {
    let mut rig = Rig::new(false, LOCKSTEP);
    assert_eq!(
        rig.routes,
        vec![vec![0, 1], vec![2, 3]],
        "insert off: one runtime, and one worker, per guitar"
    );
    rig.run(4 * WINDOW, on_time);
    let before = rig.run(WINDOW, on_time);
    let settled = rig.damage();
    let t0 = rig.cycle;
    let late_span = LATE_EVERY * LATE_BUFFERS;
    let during = rig.run(late_span + 3 * WINDOW, move |cycle, runtime| {
        runtime == 0 && cycle - t0 < late_span && (cycle - t0) % LATE_EVERY == LATE_EVERY - 1
    });
    let damage = rig.damage_since(&settled);
    for route in [2usize, 3] {
        assert_eq!(
            damage[&route],
            (0, 0),
            "guitarra-2's route {route} took (underrun frames, trims) {:?} from \
             guitarra-1's late worker",
            damage[&route]
        );
        assert_eq!(
            during[&route], before[&route],
            "guitarra-2's route {route} moved from rest {:?} to {:?} while only \
             guitarra-1's worker was late",
            before[&route], during[&route]
        );
    }
}

/// Measured: one route of the stream reached 12032 underruns while its sibling
/// had 2496. The routes one worker feeds see the same producer — a late buffer
/// is the same buffer on each — so with the output streams brought up one
/// after another, their damage may differ by at most the one buffer a deeper
/// start could absorb. Anything more is the guard's own per-route phase
/// manufacturing dropouts on one route and not the other.
#[test]
fn the_routes_of_one_worker_take_the_same_damage_from_its_late_buffers() {
    for insert_enabled in [false, true] {
        let mut rig = Rig::new(insert_enabled, STAGGERED);
        rig.run(4 * WINDOW, on_time);
        let settled = rig.damage();
        let t0 = rig.cycle;
        rig.run(LATE_EVERY * LATE_BUFFERS, move |cycle, _| {
            (cycle - t0) % LATE_EVERY == LATE_EVERY - 1
        });
        rig.run(3 * WINDOW, on_time);
        let damage = rig.damage_since(&settled);
        for (owner, routes) in rig.routes.iter().enumerate() {
            let taken: Vec<(usize, (u64, u64))> =
                routes.iter().map(|route| (*route, damage[route])).collect();
            let underruns: Vec<u64> = taken.iter().map(|(_, (u, _))| *u).collect();
            let trims: Vec<u64> = taken.iter().map(|(_, (_, t))| *t).collect();
            assert!(
                spread(&underruns) <= FRAMES as u64 && spread(&trims) <= 1,
                "insert {insert_enabled}, worker {owner}: {LATE_BUFFERS} late buffers cost \
                 its routes different damage, (route, (underrun frames, trims)) = {taken:?} — \
                 the same producer must cost every route it feeds the same"
            );
        }
    }
}

/// #953 on this rig: Out 2 of `guitarra-1` misses two callbacks while the
/// worker keeps pushing, so its ring fills. The guard sheds what the stall
/// left, once, without a single underrun, and the route returns to the rest of
/// its sibling Main — both play the same guitar, and a buffer between them is
/// the "sound stacked on the other" #953 was about. No other route is touched.
#[test]
fn an_output_stall_is_shed_once_without_starving_the_route() {
    for insert_enabled in [false, true] {
        let mut rig = Rig::new(insert_enabled, LOCKSTEP);
        let before = rig.run(8 * WINDOW, on_time);
        let settled = rig.damage();
        rig.stalled = Some(OUT_2_OF_GUITAR_1);
        rig.run(2, on_time);
        rig.stalled = None;
        rig.run(4 * WINDOW, on_time);
        let after = rig.run(WINDOW, on_time);
        assert_eq!(
            after, before,
            "insert {insert_enabled}: after Out 2's stream missed two callbacks the routes \
             rest at {after:?}, before at {before:?} — the stall's latency was kept"
        );
        for (route, (underruns, trims)) in rig.damage_since(&settled) {
            let cuts_allowed = u64::from(route == OUT_2_OF_GUITAR_1);
            assert!(
                underruns == 0 && trims <= cuts_allowed,
                "insert {insert_enabled}: route {route} was cut {trims} times and starved \
                 {underruns} frames by a two-callback stall of route {OUT_2_OF_GUITAR_1}"
            );
        }
    }
}
