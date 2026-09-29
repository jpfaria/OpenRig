//! #979 — a late DSP worker, in the shapes the owner's rig can produce: one
//! late buffer, bursts of 2 to 8, lateness that repeats, and a catch-up that
//! jitters by up to one buffer around the next cycle's output callbacks.
//!
//! Hypothesis: what degrades on the rig is the ROUTE's recovery from a late
//! #670 worker, not the worker. A late worker costs a route more than the
//! buffers it failed to deliver, keeps costing after it is back on time, or
//! leaves the route playing later than before. That is the live symptom:
//! `latency_trims` rising and underruns climbing in 64-frame multiples until
//! a rebuild while both workers are only ~28% busy, one route degrading far
//! faster than its sibling (12032 vs 2496 underrun frames), and the output
//! peaking at +10.1 dBFS.
//!
//! Intended behaviour (the owner's invariants): a worker late for N buffers
//! costs each of its routes at most those N buffers of silence; the moment it
//! is back on time the route stops losing audio for good; a trim only sheds
//! latency that is really stuck, at most once per late buffer and never once
//! the route has settled; and afterwards every route plays exactly as late as
//! it did before the event — never later, never a buffer apart from a route
//! that was in step with it, never louder. A late worker on one guitar costs
//! the other guitar nothing.
//!
//! Child module of `issue_979_two_head_rig_tests`: the topology comes from
//! there through `super::`. The HAL cycle is the parent's (#965): one IO
//! thread, the input callback, every output callback in route order, then the
//! #670 worker. Unlike the parent's `Hal`, each runtime has its own worker
//! with its own queue of the input buffers it has not processed yet, so a late
//! worker processes the real buffers it was handed, oldest first, when it
//! catches up, and one guitar's worker can be late while the other's is not.
//! The SYN-2 return is driven as an input of its own: the timing under test
//! is the route's, not the gear's.

use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::sync::Arc;

use super::{
    process_input_f32, process_output_f32, runtimes, written_routes, FRAMES, HD8_CHANNELS, RATE,
};
use crate::runtime_state::ChainRuntimeState;

/// The #953 guard judges each route once per ~186 ms window: 8192 frames,
/// 128 callbacks at the owner's 64-frame buffer (docs/audio-config.md).
const WINDOW: usize = 128;
/// Cycles played on time before anything is measured: past the cold start
/// (the routes are born empty, so the first callback underruns). ~1.5 s.
const WARM_UP: usize = 1_000;
/// What the guard gets to shed the buffers a catch-up left above the route's
/// level: the window the catch-up landed in, the next whole one, and one more
/// (~560 ms).
const SETTLE: usize = 3 * WINDOW;
/// How long a recovered route must then stay whole (~3.7 s).
const WATCH: usize = 20 * WINDOW;
/// A sample above this is heard.
const AUDIBLE: f32 = 1e-3;
/// The most a trim's crossfade may add to a route's peak. Two stacked copies
/// of one signal are +6 dB; the rig peaked at +10.1 dBFS.
const LOUDER_DB: f32 = 1.0;

/// When a stream's DSP worker pushes what it holds, relative to one HAL cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Push {
    /// After this cycle's output callbacks: the normal #670 hand-off.
    OnTime,
    /// Not before the next cycle's output callbacks: the worker is late.
    Late,
    /// A late worker catching up in the middle of this cycle's output
    /// callbacks: the buffers it was late with land after the first `n`
    /// callbacks (in route order) and before the rest; this cycle's own
    /// buffer, handed over microseconds earlier, still lands after them all.
    Midway(usize),
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
    /// Frames queued when the route's last callback started, once settled:
    /// with the worker on time, that is the route's latency.
    settled_fill: usize,
}

/// The owner's chain on one simulated HD 8.
struct Rig {
    runtimes: Vec<Arc<ChainRuntimeState>>,
    /// (runtime, route index, output channels) in output-callback order.
    routes: Vec<(usize, usize, Vec<usize>)>,
    /// Per runtime (one #670 worker each): input buffers handed over and not
    /// processed yet, oldest first.
    pending: Vec<VecDeque<Vec<f32>>>,
    out: Vec<f32>,
    /// Frames queued on each route when its last callback started.
    fill: Vec<usize>,
    /// What each route played in its last callback, its channels interleaved.
    played: Vec<Vec<f32>>,
    /// Loudest sample each route has played since the last reset.
    peak: Vec<f32>,
    /// Frames of tone generated so far.
    clock: usize,
}

fn silence() -> Vec<f32> {
    vec![0.0; FRAMES * HD8_CHANNELS]
}

/// A click on both guitars and on the insert return, in the first frame.
fn click() -> Vec<f32> {
    let mut buffer = silence();
    for ch in [0, 1, 2, 3] {
        buffer[ch] = 0.5;
    }
    buffer
}

/// Deterministic noise for the jitter (the engine's tests carry no `rand`).
struct Lcg(u64);

impl Lcg {
    /// A number in `0..n`.
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % n
    }
}

/// One cycle of workers jittering by up to one buffer: one cycle in 8 a
/// worker misses the next cycle's output callbacks, and its catch-up then
/// lands anywhere among them — before them all (it was barely late), between
/// two (a route got the buffer, its sibling did not), or after them all (a
/// whole buffer late). Never two late cycles in a row: the jitter is at most
/// one buffer. `was_late` holds, per worker, whether the last cycle was late.
fn jitter(noise: &mut Lcg, was_late: &mut [bool], callbacks: usize) -> Vec<Push> {
    was_late
        .iter_mut()
        .map(|was| {
            let push = if *was {
                Push::Midway(noise.below(callbacks + 1))
            } else if noise.below(8) == 0 {
                Push::Late
            } else {
                Push::OnTime
            };
            *was = push == Push::Late;
            push
        })
        .collect()
}

impl Rig {
    fn new(insert_enabled: bool) -> Self {
        let rts = runtimes(insert_enabled);
        let routes: Vec<(usize, usize, Vec<usize>)> = rts
            .iter()
            .enumerate()
            .flat_map(|(rt, runtime)| {
                written_routes(runtime)
                    .into_iter()
                    .map(move |(route, channels)| (rt, route, channels))
            })
            .collect();
        let n = routes.len();
        Self {
            pending: rts.iter().map(|_| VecDeque::new()).collect(),
            runtimes: rts,
            routes,
            out: vec![0.0; FRAMES * HD8_CHANNELS],
            fill: vec![0; n],
            played: vec![Vec::new(); n],
            peak: vec![0.0; n],
            clock: 0,
        }
    }

    /// The chain played on time past its cold start. Precondition of every
    /// test here: with every worker on time the chain runs clean, so whatever
    /// a test then measures is the lateness's doing.
    fn warm(insert_enabled: bool) -> Self {
        let mut rig = Self::new(insert_enabled);
        rig.run(Push::OnTime, WARM_UP);
        let warm = rig.counters();
        rig.run(Push::OnTime, 2 * WINDOW);
        for (i, (now, then)) in rig.counters().iter().zip(&warm).enumerate() {
            assert_eq!(
                (now.0 - then.0, now.1 - then.1),
                (0, 0),
                "precondition, insert {insert_enabled}: {} underran or was trimmed with \
                 every worker on time (underrun frames, trims)",
                rig.name(i)
            );
        }
        rig
    }

    fn name(&self, i: usize) -> String {
        format!("route {} {:?}", self.routes[i].1, self.routes[i].2)
    }

    /// Worker `rt` processes the buffers it holds, oldest first, leaving the
    /// newest `keep` for later.
    fn work(&mut self, rt: usize, keep: usize) {
        while self.pending[rt].len() > keep {
            if let Some(buffer) = self.pending[rt].pop_front() {
                process_input_f32(&self.runtimes[rt], 0, &buffer, HD8_CHANNELS);
            }
        }
    }

    fn route_len(&self, i: usize) -> usize {
        let (rt, route) = (self.routes[i].0, self.routes[i].1);
        let loaded = self.runtimes[rt].output_routes.load();
        let buffer = &loaded[route]
            .as_ref()
            .expect("a route the chain writes")
            .buffer;
        buffer.len()
    }

    /// (underrun frames, latency trims) of every route, in callback order.
    fn counters(&self) -> Vec<(u64, u64)> {
        (0..self.routes.len())
            .map(|i| {
                let (rt, route) = (self.routes[i].0, self.routes[i].1);
                let loaded = self.runtimes[rt].output_routes.load();
                let buffer = &loaded[route]
                    .as_ref()
                    .expect("a route the chain writes")
                    .buffer;
                (buffer.underrun_count(), buffer.latency_trims())
            })
            .collect()
    }

    /// One HAL cycle: the input callback hands `input` to every worker, every
    /// route's output callback runs, and each worker pushes as `schedule`
    /// (one entry per runtime) says. Returns, per route, the first frame of
    /// this cycle it played audibly.
    fn cycle(&mut self, schedule: &[Push], input: &[f32]) -> Vec<Option<usize>> {
        assert_eq!(schedule.len(), self.runtimes.len(), "one push per worker");
        for queue in &mut self.pending {
            queue.push_back(input.to_vec());
        }
        let mut heard = vec![None; self.routes.len()];
        for i in 0..self.routes.len() {
            for (rt, push) in schedule.iter().enumerate() {
                if *push == Push::Midway(i) {
                    self.work(rt, 1);
                }
            }
            let (rt, route) = (self.routes[i].0, self.routes[i].1);
            let fill = self.route_len(i);
            self.fill[i] = fill;
            self.out.fill(0.0);
            process_output_f32(&self.runtimes[rt], route, &mut self.out, HD8_CHANNELS);
            let channels = &self.routes[i].2;
            let played = &mut self.played[i];
            played.clear();
            for (f, frame) in self.out.chunks_exact(HD8_CHANNELS).enumerate() {
                for &ch in channels {
                    let sample = frame[ch];
                    played.push(sample);
                    self.peak[i] = self.peak[i].max(sample.abs());
                    if heard[i].is_none() && sample.abs() > AUDIBLE {
                        heard[i] = Some(f);
                    }
                }
            }
        }
        for (rt, push) in schedule.iter().enumerate() {
            if *push != Push::Late {
                self.work(rt, 0);
            }
        }
        heard
    }

    /// One cycle of silence.
    fn step(&mut self, schedule: &[Push]) {
        let input = silence();
        self.cycle(schedule, &input);
    }

    /// `cycles` cycles of silence with every worker pushing as `push`.
    fn run(&mut self, push: Push, cycles: usize) {
        let schedule = vec![push; self.runtimes.len()];
        for _ in 0..cycles {
            self.step(&schedule);
        }
    }

    /// A steady tone: a different pitch on each guitar and on the insert
    /// return, 0.25 each, continuous across cycles.
    fn tone(&mut self) -> Vec<f32> {
        let mut buffer = silence();
        for frame in buffer.chunks_mut(HD8_CHANNELS) {
            let t = (self.clock % 44_100) as f32 / RATE;
            frame[0] = 0.25 * (TAU * 220.0 * t).sin();
            frame[1] = 0.25 * (TAU * 330.0 * t).sin();
            frame[2] = 0.25 * (TAU * 440.0 * t).sin();
            frame[3] = frame[2];
            self.clock += 1;
        }
        buffer
    }

    /// `cycles` cycles of tone.
    fn play(&mut self, schedule: &[Push], cycles: usize) {
        for _ in 0..cycles {
            let input = self.tone();
            self.cycle(schedule, &input);
        }
    }

    /// Frames from a click at the inputs to its first audible sample, per
    /// route, every worker on time. Plays the click out completely.
    fn click_latency(&mut self) -> Vec<usize> {
        let schedule = vec![Push::OnTime; self.runtimes.len()];
        let mut latency: Vec<Option<usize>> = vec![None; self.routes.len()];
        for cycle in 0..64 {
            let input = if cycle == 0 { click() } else { silence() };
            let heard = self.cycle(&schedule, &input);
            for (i, frame) in heard.into_iter().enumerate() {
                if latency[i].is_none() {
                    latency[i] = frame.map(|f| cycle * FRAMES + f);
                }
            }
        }
        latency
            .into_iter()
            .enumerate()
            .map(|(i, l)| l.unwrap_or_else(|| panic!("{} never played the click", self.name(i))))
            .collect()
    }

    /// Runs `late` (the event, up to and including the cycle whose push
    /// catches up), then plays on time: SETTLE cycles for the guard to shed
    /// what the catch-up left behind, then `watch` more. Returns what the
    /// event cost every route.
    fn event(&mut self, late: impl FnOnce(&mut Rig), watch: usize) -> Vec<Outcome> {
        let start = self.counters();
        late(self);
        let back = self.counters();
        self.run(Push::OnTime, SETTLE);
        let settled = self.counters();
        let settled_fill = self.fill.clone();
        self.run(Push::OnTime, watch);
        let end = self.counters();
        (0..self.routes.len())
            .map(|i| Outcome {
                lost_while_late: back[i].0 - start[i].0,
                lost_after: end[i].0 - back[i].0,
                trims_to_settle: settled[i].1 - start[i].1,
                trims_after: end[i].1 - settled[i].1,
                settled_fill: settled_fill[i],
            })
            .collect()
    }

    /// The owner's invariants on one event, per route: at most `late[w]`
    /// buffers of silence while its worker `w` was late (a late buffer costs
    /// at most itself), not one frame once the worker is back on time, at
    /// most `trims[w]` trims to shed what the catch-up left behind and none
    /// once settled, back at — never above — the fill it rested at before,
    /// and still in step with every route of its stream it was in step with.
    fn check(&self, what: &str, outcome: &[Outcome], late: &[u64], trims: &[u64], rest: &[usize]) {
        let frames = FRAMES as u64;
        for (i, o) in outcome.iter().enumerate() {
            let (worker, route) = (self.routes[i].0, self.name(i));
            assert!(
                o.lost_while_late <= late[worker] * frames,
                "{what}, {route}: {} late buffer(s) of its worker cost {} underrun frames, \
                 more than the {} frames the worker failed to deliver",
                late[worker],
                o.lost_while_late,
                late[worker] * frames
            );
            assert_eq!(
                o.lost_after, 0,
                "{what}, {route}: the worker was back on time, yet the route kept starving \
                 ({} more underrun frames) — the rig's underruns climbing in 64-frame \
                 multiples until a rebuild",
                o.lost_after
            );
            assert!(
                o.trims_to_settle <= trims[worker],
                "{what}, {route}: {} trims to recover from {} late buffer(s); at most {} \
                 sheds what the catch-up really left behind",
                o.trims_to_settle,
                late[worker],
                trims[worker]
            );
            assert_eq!(
                o.trims_after, 0,
                "{what}, {route}: {} trims after the route had settled with its worker on \
                 time — its latency was not stuck, the guard cut its cushion (the rig: \
                 `latency_trims` rising until a rebuild)",
                o.trims_after
            );
            assert!(
                o.settled_fill <= rest[i],
                "{what}, {route}: settled at {} queued frames, {} before the event — a late \
                 worker must never leave a route later than it was",
                o.settled_fill,
                rest[i]
            );
        }
        for i in 0..outcome.len() {
            for j in i + 1..outcome.len() {
                if self.routes[i].0 == self.routes[j].0 && rest[i] == rest[j] {
                    assert_eq!(
                        outcome[i].settled_fill,
                        outcome[j].settled_fill,
                        "{what}: {} and {} rested in step at {} frames; now {} vs {} — one \
                         stream a buffer apart on two outputs (the rig: Main at 64 frames on \
                         one route and 128 on its sibling, heard as stacked streams)",
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

    /// No route plays a click later than it did before, and routes of one
    /// stream that were in step still are.
    fn check_latency(&self, what: &str, before: &[usize], after: &[usize]) {
        for i in 0..self.routes.len() {
            assert!(
                after[i] <= before[i],
                "{what}, {}: a click now takes {} frames, {} before — route latency must \
                 never grow",
                self.name(i),
                after[i],
                before[i]
            );
            for j in i + 1..self.routes.len() {
                if self.routes[i].0 == self.routes[j].0 && before[i] == before[j] {
                    assert_eq!(
                        after[i],
                        after[j],
                        "{what}: {} and {} played a click together before; now {} vs {} \
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

/// One late buffer, at every position of the guard's window: each route
/// loses at most that one buffer, nothing after the catch-up, sheds the
/// buffer the catch-up left behind at most once, and plays exactly as late as
/// before. The parent's `a_late_worker_costs_one_buffer_and_never_starts_a_stutter`
/// counts totals over six minutes; this judges every event on its own.
#[test]
fn one_late_buffer_at_any_window_phase_costs_that_buffer_and_nothing_after() {
    // Late cycle + catch-up + SETTLE + watch = 7 windows + 1: every event
    // lands one callback later in the window than the one before.
    const WATCH_ONE: usize = 4 * WINDOW - 1;
    for insert_enabled in [false, true] {
        let mut rig = Rig::warm(insert_enabled);
        let before = rig.click_latency();
        let rest = rig.fill.clone();
        let workers = rig.runtimes.len();
        for phase in 0..WINDOW {
            let outcome = rig.event(
                |rig| {
                    rig.run(Push::Late, 1);
                    rig.run(Push::OnTime, 1);
                },
                WATCH_ONE,
            );
            rig.check(
                &format!("insert {insert_enabled}, one late buffer at window phase {phase}"),
                &outcome,
                &vec![1; workers],
                &vec![1; workers],
                &rest,
            );
        }
        let after = rig.click_latency();
        rig.check_latency(
            &format!("insert {insert_enabled}, after {WINDOW} single late buffers"),
            &before,
            &after,
        );
    }
}

/// A burst of 2 to 8 late buffers (a worker stalled for up to 11.6 ms)
/// costs each route at most the burst, then nothing: whatever the catch-up
/// piled up above the route's cushion is shed once, and the route plays
/// exactly as late as before.
#[test]
fn a_burst_of_2_to_8_late_buffers_costs_the_burst_and_nothing_after() {
    const EVENTS: usize = 16;
    for burst in 2..=8usize {
        for insert_enabled in [false, true] {
            let mut rig = Rig::warm(insert_enabled);
            let before = rig.click_latency();
            let rest = rig.fill.clone();
            let workers = rig.runtimes.len();
            for event in 0..EVENTS {
                // Burst + catch-up + SETTLE + watch = 7 windows + 17: 17 is
                // coprime with the window, so the bursts walk its phases.
                let outcome = rig.event(
                    |rig| {
                        rig.run(Push::Late, burst);
                        rig.run(Push::OnTime, 1);
                    },
                    4 * WINDOW + 16 - burst,
                );
                rig.check(
                    &format!("insert {insert_enabled}, burst of {burst}, event {event}"),
                    &outcome,
                    &vec![burst as u64; workers],
                    &vec![1; workers],
                    &rest,
                );
            }
            let after = rig.click_latency();
            rig.check_latency(
                &format!("insert {insert_enabled}, after {EVENTS} bursts of {burst}"),
                &before,
                &after,
            );
        }
    }
}

/// Lateness that repeats: every other buffer, every third, a few times per
/// window, and once per window drifting across its phases (127 and 129
/// cycles), for 6 to 24 s. While it lasts, each late buffer costs a route at
/// most itself and at most one trim: the route may hold the cushion a late
/// worker needs, never lose audio that keeps coming. Once the worker is back
/// on time for good, the route stops losing audio at once and returns to its
/// latency.
#[test]
fn lateness_that_repeats_costs_each_late_buffer_at_most_once_and_stops_when_it_stops() {
    for period in [2usize, 3, 16, 127, 129] {
        for insert_enabled in [false, true] {
            let mut rig = Rig::warm(insert_enabled);
            let before = rig.click_latency();
            let rest = rig.fill.clone();
            let workers = rig.runtimes.len();
            let cycles = (period * WINDOW).max(32 * WINDOW);
            let late = (cycles / period) as u64;
            let outcome = rig.event(
                |rig| {
                    for cycle in 1..=cycles {
                        let push = if cycle % period == 0 {
                            Push::Late
                        } else {
                            Push::OnTime
                        };
                        rig.run(push, 1);
                    }
                    rig.run(Push::OnTime, 1);
                },
                WATCH,
            );
            rig.check(
                &format!(
                    "insert {insert_enabled}, late every {period} cycles ({late} late buffers)"
                ),
                &outcome,
                &vec![late; workers],
                &vec![late; workers],
                &rest,
            );
            let after = rig.click_latency();
            rig.check_latency(
                &format!("insert {insert_enabled}, after lateness every {period} cycles"),
                &before,
                &after,
            );
        }
    }
}

/// Lateness at the SAME position of the guard's window, three windows in a
/// row, for every position: a worker that is late on the callback that
/// closes a window (the ring then looks empty to a window that has not
/// counted that callback's underrun yet) must not teach the route a level it
/// then keeps cutting to once the worker is on time.
#[test]
fn lateness_at_the_same_window_phase_every_window_stops_costing_when_it_stops() {
    const REPEATS: usize = 3;
    // REPEATS windows + SETTLE + watch = 8 windows + 1: each phase is one
    // callback later than the one before.
    const WATCH_PHASE: usize = 2 * WINDOW + 1;
    for insert_enabled in [false, true] {
        let mut rig = Rig::warm(insert_enabled);
        let before = rig.click_latency();
        let rest = rig.fill.clone();
        let workers = rig.runtimes.len();
        for phase in 0..WINDOW {
            let outcome = rig.event(
                |rig| {
                    for _ in 0..REPEATS {
                        rig.run(Push::Late, 1);
                        rig.run(Push::OnTime, WINDOW - 1);
                    }
                },
                WATCH_PHASE,
            );
            rig.check(
                &format!(
                    "insert {insert_enabled}, late at window phase {phase} for {REPEATS} windows"
                ),
                &outcome,
                &vec![REPEATS as u64; workers],
                &vec![REPEATS as u64; workers],
                &rest,
            );
        }
        let after = rig.click_latency();
        rig.check_latency(
            &format!("insert {insert_enabled}, after late windows at every phase"),
            &before,
            &after,
        );
    }
}

/// A late push that lands between two output callbacks of the next cycle:
/// the routes whose callback came first underran, their siblings of the same
/// stream played the buffer. Each route loses at most that buffer, and the
/// one that underran must not stay a buffer behind its sibling — the rig's
/// Main routes sat at 64 and 128 frames, and one route of a stream degraded
/// almost five times faster than its sibling (12032 vs 2496 underrun frames).
#[test]
fn a_late_push_landing_between_output_callbacks_leaves_no_route_behind_its_sibling() {
    const EVENTS: usize = 8;
    for insert_enabled in [false, true] {
        let mut rig = Rig::warm(insert_enabled);
        let before = rig.click_latency();
        let rest = rig.fill.clone();
        let workers = rig.runtimes.len();
        for landing in 1..rig.routes.len() {
            let midway = vec![Push::Midway(landing); workers];
            for event in 0..EVENTS {
                // Late + catch-up + SETTLE + watch = 7 windows + 17.
                let outcome = rig.event(
                    |rig| {
                        rig.run(Push::Late, 1);
                        rig.step(&midway);
                    },
                    4 * WINDOW + 15,
                );
                rig.check(
                    &format!(
                        "insert {insert_enabled}, late push landing after {landing} output \
                         callback(s), event {event}"
                    ),
                    &outcome,
                    &vec![1; workers],
                    &vec![1; workers],
                    &rest,
                );
            }
        }
        let after = rig.click_latency();
        rig.check_latency(
            &format!("insert {insert_enabled}, after late pushes landing between callbacks"),
            &before,
            &after,
        );
    }
}

/// Workers whose push jitters by up to one buffer for ~12 s (one cycle in 8
/// late, the catch-up landing anywhere among the next cycle's callbacks):
/// each late buffer costs its routes at most itself and one trim, and once
/// the jitter stops the routes lose nothing more, stop trimming and play
/// exactly as late as before.
#[test]
fn a_push_jittering_by_one_buffer_costs_at_most_its_late_buffers_and_nothing_after() {
    const JITTER: usize = 64 * WINDOW;
    for seed in [1u64, 979, 0xdead_beef] {
        for insert_enabled in [false, true] {
            let mut rig = Rig::warm(insert_enabled);
            let before = rig.click_latency();
            let rest = rig.fill.clone();
            let workers = rig.runtimes.len();
            let callbacks = rig.routes.len();
            let mut noise = Lcg(seed);
            let mut late = vec![0u64; workers];
            let outcome = rig.event(
                |rig| {
                    let mut was_late = vec![false; workers];
                    for _ in 0..JITTER {
                        let schedule = jitter(&mut noise, &mut was_late, callbacks);
                        for (w, push) in schedule.iter().enumerate() {
                            late[w] += u64::from(*push == Push::Late);
                        }
                        rig.step(&schedule);
                    }
                    rig.run(Push::OnTime, 1);
                },
                WATCH,
            );
            rig.check(
                &format!("insert {insert_enabled}, one-buffer jitter (seed {seed}, late {late:?})"),
                &outcome,
                &late,
                &late,
                &rest,
            );
            let after = rig.click_latency();
            rig.check_latency(
                &format!("insert {insert_enabled}, after one-buffer jitter (seed {seed})"),
                &before,
                &after,
            );
        }
    }
}

/// Insert off, the chain is two isolated streams, one #670 worker each. With
/// `guitarra-1`'s worker late — bursts of 1 to 8, then ~3 s of one-buffer
/// jitter — `guitarra-2`'s routes play, sample for sample, what they play
/// when nobody is late, and lose, trim and move nothing: N streams are N
/// pipelines that do not know the others exist.
// linux+JACK keeps the pre-#967 grouping (one runtime per chain).
#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn a_late_worker_on_one_guitar_costs_the_other_guitar_nothing() {
    let mut reference = Rig::warm(false);
    let mut rig = Rig::warm(false);
    assert_eq!(
        rig.runtimes.len(),
        2,
        "insert off: one runtime (one worker) per guitar"
    );
    let before = rig.click_latency();
    reference.click_latency();
    let rest = rig.fill.clone();
    let other: Vec<usize> = (0..rig.routes.len())
        .filter(|&i| rig.routes[i].0 == 1)
        .collect();
    assert_eq!(other.len(), 2, "guitarra-2 writes Main and Out 2");

    let mut schedules: Vec<[Push; 2]> = Vec::new();
    for burst in 1..=8 {
        schedules.extend(std::iter::repeat([Push::Late, Push::OnTime]).take(burst));
        schedules.extend(std::iter::repeat([Push::OnTime, Push::OnTime]).take(2 * WINDOW + 17));
    }
    let mut noise = Lcg(979);
    let mut was_late = [false];
    for _ in 0..16 * WINDOW {
        let first = jitter(&mut noise, &mut was_late, rig.routes.len())[0];
        schedules.push([first, Push::OnTime]);
    }
    schedules.push([Push::OnTime, Push::OnTime]);
    let late = schedules.iter().filter(|s| s[0] == Push::Late).count() as u64;

    let outcome = rig.event(
        |rig| {
            for (cycle, schedule) in schedules.iter().enumerate() {
                let input = rig.tone();
                rig.cycle(schedule, &input);
                reference.cycle(&[Push::OnTime; 2], &input);
                for &i in &other {
                    assert!(
                        rig.played[i] == reference.played[i],
                        "cycle {cycle}: guitarra-2's {} played something else because \
                         guitarra-1's worker was late — one stream's lateness reached another",
                        rig.name(i)
                    );
                }
            }
        },
        WATCH,
    );
    rig.check(
        "insert off, guitarra-1's worker late, guitarra-2's on time",
        &outcome,
        &[late, 0],
        &[late, 0],
        &rest,
    );
    let after = rig.click_latency();
    rig.check_latency("insert off, one guitar's worker late", &before, &after);
}

/// However the worker is late — a single buffer, a burst of up to 8, or
/// ~3 s of one-buffer jitter — no route ever plays louder than it does on
/// time: a late buffer is silence (#496), a catch-up plays each frame once,
/// and a trim crossfades between two parts of one signal. Nothing sums two
/// copies (the rig peaked at +10.1 dBFS; per-stream volume is immutable).
#[test]
fn a_late_worker_never_makes_a_route_louder() {
    for insert_enabled in [false, true] {
        let mut rig = Rig::warm(insert_enabled);
        let workers = rig.runtimes.len();
        let callbacks = rig.routes.len();
        let on_time = vec![Push::OnTime; workers];
        rig.play(&on_time, 4 * WINDOW);
        rig.peak.fill(0.0);
        rig.play(&on_time, 4 * WINDOW);
        let baseline = rig.peak.clone();
        for (i, peak) in baseline.iter().enumerate() {
            assert!(
                *peak > AUDIBLE,
                "precondition, insert {insert_enabled}: the tone reaches {}",
                rig.name(i)
            );
        }
        rig.peak.fill(0.0);
        for burst in 1..=8 {
            rig.play(&vec![Push::Late; workers], burst);
            rig.play(&on_time, SETTLE + 17);
        }
        let mut noise = Lcg(0x979);
        let mut was_late = vec![false; workers];
        for _ in 0..16 * WINDOW {
            let schedule = jitter(&mut noise, &mut was_late, callbacks);
            rig.play(&schedule, 1);
        }
        rig.play(&on_time, SETTLE);
        let limit = 10f32.powf(LOUDER_DB / 20.0);
        for (i, (peak, base)) in rig.peak.iter().zip(&baseline).enumerate() {
            assert!(
                *peak <= base * limit,
                "insert {insert_enabled}, {}: peaked at {:+.1} dB against its on-time \
                 {:.3} — a late worker made the route louder",
                rig.name(i),
                20.0 * (peak / base).log10(),
                base
            );
        }
    }
}
