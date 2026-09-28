//! #979 — hypothesis: a scene switch or an insert toggle through the #967 fast
//! path carries the previous scene into the next one.
//!
//! The fast path keeps what is live. The runtime graph re-upserts the chain
//! into the entries it already holds (#316), a chain holding a VST3 is updated
//! in place runtime by runtime (#779), and both reuse the routes' rings (#670,
//! #969). On the owner's rig the insert also REGROUPS the heads: with the
//! SYN-2 loop on, both guitars are one pipeline (one runtime); with it off,
//! each guitar is its own runtime (#703). A switch that keeps what it should
//! have replaced could leave:
//!
//! - a head in two live runtimes: the same guitar processed twice, which is
//!   "several streams stacked" and a +10.1 dBFS peak;
//! - a route owned by a runtime whose pipelines no longer write it: its stream
//!   pops it empty on every callback, so underruns climb in whole 64-frame
//!   buffers until the chain is rebuilt;
//! - the send `[3]` still written with the loop off;
//! - a ring holding fill the new scene never produced: latency that each
//!   switch adds to and that only a rebuild removes.
//!
//! Each test drives the owner's topology through K switches: the parent's
//! `rig_chain` and `rig_registry`, plus two stand-ins for the VST3 reverbs.
//! The result is held to what a fresh build of the same scene is. That is the
//! owner's workaround (chain off and on), and it is what "fixes it" live.
//!
//! The HAL cycle is the parent's, as #965 measured it on the HD 8. The input
//! callback runs first, then every route's output callback (each route is its
//! own output stream and the device sums them), then the #670 worker, always on
//! time here. The SYN-2 loop is closed: what the send played on channel 3 in
//! one cycle comes back on In 3/4 in the next.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

use std::collections::{BTreeMap, BTreeSet, HashMap};
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

/// Footswitch presses of the insert alone.
const TOGGLES: usize = 10;
/// Scene switches per run on the in-place (VST3) path.
const IN_PLACE_SWITCHES: usize = 12;
/// HAL cycles a switch is judged over: four drift-guard windows (128
/// callbacks each at 64 frames, ~186 ms).
const POST: usize = 512;
/// HAL cycles between two in-place switches (~93 ms).
const BETWEEN: usize = 64;
/// HAL cycles that let the rig settle before its structure is read.
const SETTLE: usize = 8;
/// Cycles that flush what played before a level is measured: the rings, the
/// loop and any rebuild fade.
const FLUSH: usize = 32;
/// Cycles a level is measured over (~140 ms, about 30 periods of the tone).
const MEASURE: usize = 96;
/// −80 dBFS.
const SILENT: f32 = 1e-4;
/// A level further than this from a fresh build is not the same stream. A
/// doubled head is +6 dB.
const LEVEL_TOLERANCE_DB: f32 = 0.5;

#[derive(Clone, Copy, Debug)]
struct Scene {
    insert: bool,
    preamp: bool,
    reverb_a: bool,
    reverb_b: bool,
}

const fn scene(insert: bool, preamp: bool, reverb_a: bool, reverb_b: bool) -> Scene {
    Scene {
        insert,
        preamp,
        reverb_a,
        reverb_b,
    }
}

/// The scene of the report: the SYN-2 in the loop, the preamp off, both
/// reverbs off.
const SYN2: Scene = scene(true, false, false, false);
/// `ANAL+DIG`, the scene active in the live capture: insert off, the NAM
/// preamp on.
const ANAL_DIG: Scene = scene(false, true, false, false);
// The other on/off combinations a scene can hold.
const LOOP_REVERBS: Scene = scene(true, false, true, true);
const LOOP_PREAMP: Scene = scene(true, true, false, true);
const DRY: Scene = scene(false, false, false, false);
const PREAMP_REVERB: Scene = scene(false, true, true, false);

/// K = 12 switches starting from `SYN2`. Eight of them flip the insert, which
/// regroups the heads on this rig and is therefore structural (#967). The other
/// four keep the insert where it was and go in place.
const SCENE_SWITCHES: [Scene; 12] = [
    ANAL_DIG,      // flip
    DRY,           // keep
    SYN2,          // flip
    LOOP_REVERBS,  // keep
    ANAL_DIG,      // flip
    LOOP_PREAMP,   // flip
    SYN2,          // keep
    DRY,           // flip
    PREAMP_REVERB, // keep
    SYN2,          // flip
    ANAL_DIG,      // flip
    SYN2,          // flip
];

/// Scenes a VST3 chain switches between in place (#779). The insert stays
/// where it is, and so does the grouping.
const LOOP_ON_SCENES: [Scene; 3] = [SYN2, LOOP_REVERBS, LOOP_PREAMP];
const LOOP_OFF_SCENES: [Scene; 3] = [ANAL_DIG, DRY, PREAMP_REVERB];

/// `rig:input-7` in `scene`: the parent's chain (insert, preamp, cab) plus the
/// two VST3 reverbs, stood in by volume blocks like the preamp and the cab.
fn chain_for(scene: Scene) -> Chain {
    let mut chain = rig_chain(scene.insert);
    chain.blocks.push(gain("reverb-a"));
    chain.blocks.push(gain("reverb-b"));
    for block in chain.blocks.iter_mut() {
        match block.id.0.as_str() {
            "preamp" => block.enabled = scene.preamp,
            "reverb-a" => block.enabled = scene.reverb_a,
            "reverb-b" => block.enabled = scene.reverb_b,
            _ => {}
        }
    }
    chain
}

fn chain_id() -> ChainId {
    rig_chain(false).id
}

/// The call the controller re-upserts a live chain through (#316).
/// `needs_stream_rebuild` is true for a switch that regroups the runtimes
/// (io_topology_tests::a_switch_that_regroups_the_chains_runtimes_is_a_structural_change).
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

/// A steady 220 Hz guitar at −20 dBFS. It wraps every second, on a whole
/// number of periods.
fn tone(n: usize) -> f32 {
    0.1 * (2.0 * std::f32::consts::PI * 220.0 * (n % 44_100) as f32 / RATE).sin()
}

#[derive(Clone, Copy)]
enum Play {
    Silence,
    Guitar1,
    Guitar2,
    Both,
}

/// The owner's HD 8 with `rig:input-7` live in a runtime graph.
struct Rig {
    graph: RuntimeGraph,
    scene: Scene,
    input: Vec<f32>,
    /// What the pedals return on In 3/4: channel 3 of the previous cycle.
    pedal: Vec<f32>,
    clock: usize,
}

impl Rig {
    /// The chain switched on: a fresh build.
    fn start(scene: Scene) -> Self {
        let mut graph = RuntimeGraph {
            chains: HashMap::new(),
        };
        upsert(&mut graph, scene, true);
        Self {
            graph,
            scene,
            input: vec![0.0; FRAMES * HD8_CHANNELS],
            pedal: vec![0.0; FRAMES],
            clock: 0,
        }
    }

    /// Every runtime of the chain the graph holds, by group.
    fn live(&self) -> Vec<(usize, Arc<ChainRuntimeState>)> {
        let mut live = self.graph.runtimes_with_groups_for(&chain_id());
        live.sort_by_key(|(group, _)| *group);
        live
    }

    /// A scene switch through the graph. It is structural when it flips the
    /// insert, and in place otherwise.
    fn switch(&mut self, next: Scene) {
        upsert(&mut self.graph, next, next.insert != self.scene.insert);
        self.scene = next;
    }

    /// A scene switch on a chain holding a VST3 (#779): every live runtime is
    /// updated in place.
    fn switch_in_place(&mut self, next: Scene) {
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
        self.scene = next;
    }

    /// One HAL cycle. Returns the device's output buffer: every route's stream
    /// summed, as the HD 8 sums them.
    fn cycle(&mut self, play: Play) -> Vec<f32> {
        let (guitar_1, guitar_2) = match play {
            Play::Silence => (false, false),
            Play::Guitar1 => (true, false),
            Play::Guitar2 => (false, true),
            Play::Both => (true, true),
        };
        for (i, frame) in self.input.chunks_mut(HD8_CHANNELS).enumerate() {
            let sample = tone(self.clock * FRAMES + i);
            frame[0] = if guitar_1 { sample } else { 0.0 };
            frame[1] = if guitar_2 { sample } else { 0.0 };
            frame[2] = self.pedal[i];
            frame[3] = self.pedal[i];
        }
        self.clock += 1;
        let live = self.live();
        let mut device = vec![0.0_f32; FRAMES * HD8_CHANNELS];
        for (_, runtime) in &live {
            for (route, _) in written_routes(runtime) {
                let mut stream = vec![0.0_f32; FRAMES * HD8_CHANNELS];
                process_output_f32(runtime, route, &mut stream, HD8_CHANNELS);
                for (out, sample) in device.iter_mut().zip(&stream) {
                    *out += sample;
                }
            }
        }
        for (_, runtime) in &live {
            process_input_f32(runtime, 0, &self.input, HD8_CHANNELS);
        }
        for (pedal, frame) in self.pedal.iter_mut().zip(device.chunks(HD8_CHANNELS)) {
            *pedal = frame[3];
        }
        device
    }

    fn run(&mut self, play: Play, cycles: usize) {
        for _ in 0..cycles {
            self.cycle(play);
        }
    }

    /// Peak of every physical channel while `play` plays, once what played
    /// before has been flushed.
    fn peaks(&mut self, play: Play) -> Vec<f32> {
        self.run(Play::Silence, FLUSH);
        self.run(play, FLUSH);
        let mut peaks = vec![0.0_f32; HD8_CHANNELS];
        for _ in 0..MEASURE {
            let device = self.cycle(play);
            for frame in device.chunks(HD8_CHANNELS) {
                for (peak, sample) in peaks.iter_mut().zip(frame) {
                    *peak = peak.max(sample.abs());
                }
            }
        }
        peaks
    }

    /// Plays both guitars for `cycles` cycles and reports every route's health
    /// over them.
    fn health(&mut self, cycles: usize) -> BTreeMap<usize, RouteHealth> {
        let start = route_counters(&self.live());
        let mut health: BTreeMap<usize, RouteHealth> = BTreeMap::new();
        for _ in 0..cycles {
            self.cycle(Play::Both);
            for (route, (fill, _, _)) in route_counters(&self.live()) {
                let entry = health.entry(route).or_default();
                entry.max_fill = entry.max_fill.max(fill);
            }
        }
        for (route, (_, underruns, trims)) in route_counters(&self.live()) {
            let (_, underruns_before, trims_before) =
                start.get(&route).copied().unwrap_or_default();
            let entry = health.entry(route).or_default();
            entry.underruns = underruns.saturating_sub(underruns_before);
            entry.trims = trims.saturating_sub(trims_before);
        }
        health
    }
}

/// A fresh build of `scene` after it has played a few cycles.
fn fresh_live(scene: Scene) -> Vec<(usize, Arc<ChainRuntimeState>)> {
    let mut rig = Rig::start(scene);
    rig.run(Play::Both, SETTLE);
    rig.live()
}

/// How one route fared over a stretch of cycles.
#[derive(Debug, Default)]
struct RouteHealth {
    /// Most frames queued after a worker push: the route's latency.
    max_fill: usize,
    underruns: u64,
    trims: u64,
}

/// Per route index, summed over every live runtime that owns it: (frames
/// queued, underrun frames, latency trims).
fn route_counters(live: &[(usize, Arc<ChainRuntimeState>)]) -> BTreeMap<usize, (usize, u64, u64)> {
    let mut counters: BTreeMap<usize, (usize, u64, u64)> = BTreeMap::new();
    for (_, runtime) in live {
        for (route, state) in runtime.output_routes.load().iter().enumerate() {
            if let Some(state) = state {
                let entry = counters.entry(route).or_default();
                entry.0 += state.buffer.len();
                entry.1 += state.buffer.underrun_count();
                entry.2 += state.buffer.latency_trims();
            }
        }
    }
    counters
}

fn accumulate(total: &mut BTreeMap<usize, RouteHealth>, window: BTreeMap<usize, RouteHealth>) {
    for (route, health) in window {
        let entry = total.entry(route).or_default();
        entry.max_fill = entry.max_fill.max(health.max_fill);
        entry.underruns += health.underruns;
        entry.trims += health.trims;
    }
}

/// Routes where `heard` is worse than `reference`: owned on one side only,
/// more frames queued (latency), more underrun frames or more trims.
fn worse_than(
    heard: &BTreeMap<usize, RouteHealth>,
    reference: &BTreeMap<usize, RouteHealth>,
) -> Vec<String> {
    let routes: BTreeSet<usize> = heard.keys().chain(reference.keys()).copied().collect();
    routes
        .into_iter()
        .filter_map(|route| match (heard.get(&route), reference.get(&route)) {
            (Some(h), Some(r))
                if h.max_fill <= r.max_fill && h.underruns <= r.underruns && h.trims <= r.trims =>
            {
                None
            }
            (h, r) => Some(format!("route {route}: {h:?} vs {r:?}")),
        })
        .collect()
}

/// One runtime as a fresh build can be compared with: group, pipelines (input
/// channels → routes written, sorted), owned routes with their channels, and
/// how many pipelines its input callback dispatches.
type RuntimeLayout = (
    usize,
    Vec<(Vec<usize>, Vec<usize>)>,
    Vec<(usize, Vec<usize>)>,
    usize,
);

fn layout(live: &[(usize, Arc<ChainRuntimeState>)]) -> Vec<RuntimeLayout> {
    live.iter()
        .map(|(group, runtime)| {
            let mut lanes = pipelines(runtime);
            lanes.sort();
            let dispatched = runtime
                .processing
                .lock()
                .unwrap()
                .input_to_segments
                .iter()
                .map(|segments| segments.len())
                .sum::<usize>();
            (*group, lanes, written_routes(runtime), dispatched)
        })
        .collect()
}

/// Per route index, the input channels of every pipeline that writes it,
/// across all live runtimes.
fn writers(live: &[(usize, Arc<ChainRuntimeState>)]) -> BTreeMap<usize, Vec<Vec<usize>>> {
    let mut writers: BTreeMap<usize, Vec<Vec<usize>>> = BTreeMap::new();
    for (_, runtime) in live {
        for (input, routes) in pipelines(runtime) {
            for route in routes {
                writers.entry(route).or_default().push(input.clone());
            }
        }
    }
    for inputs in writers.values_mut() {
        inputs.sort();
    }
    writers
}

/// State two live runtimes share: the same runtime, or the same ring.
fn shared_state(live: &[(usize, Arc<ChainRuntimeState>)]) -> Vec<String> {
    let mut shared = Vec::new();
    for (i, (group_a, a)) in live.iter().enumerate() {
        for (group_b, b) in &live[i + 1..] {
            if Arc::ptr_eq(a, b) {
                shared.push(format!("groups {group_a} and {group_b} are one runtime"));
                continue;
            }
            let (routes_a, routes_b) = (a.output_routes.load(), b.output_routes.load());
            for (route, (x, y)) in routes_a.iter().zip(routes_b.iter()).enumerate() {
                if let (Some(x), Some(y)) = (x, y) {
                    if Arc::ptr_eq(x, y) {
                        shared.push(format!(
                            "groups {group_a} and {group_b} share route {route}'s ring"
                        ));
                    }
                }
            }
        }
    }
    shared
}

/// (group, route) of every live route on the send channel `[3]`.
fn send_routes(live: &[(usize, Arc<ChainRuntimeState>)]) -> Vec<(usize, usize)> {
    let mut sends = Vec::new();
    for (group, runtime) in live {
        for (route, channels) in written_routes(runtime) {
            if channels == [3] {
                sends.push((*group, route));
            }
        }
    }
    sends
}

/// (group, route, frames queued) of every ring the live runtimes own.
fn ring_fill(live: &[(usize, Arc<ChainRuntimeState>)]) -> Vec<(usize, usize, usize)> {
    let mut fill = Vec::new();
    for (group, runtime) in live {
        for (route, state) in runtime.output_routes.load().iter().enumerate() {
            if let Some(state) = state {
                fill.push((*group, route, state.buffer.len()));
            }
        }
    }
    fill
}

/// Adds to `failures` every channel where `rig` plays a guitar differently
/// from a fresh build of its scene.
fn compare_levels(rig: &mut Rig, when: &str, failures: &mut Vec<String>) {
    let mut fresh = Rig::start(rig.scene);
    for (play, name) in [(Play::Guitar1, "guitarra-1"), (Play::Guitar2, "guitarra-2")] {
        let reference = fresh.peaks(play);
        assert!(
            reference[0] > SILENT,
            "setup: a fresh build of {:?} must play {name} on Main L (peak {})",
            rig.scene,
            reference[0]
        );
        let heard = rig.peaks(play);
        for (channel, (h, r)) in heard.iter().zip(&reference).enumerate() {
            let differs = match (*h > SILENT, *r > SILENT) {
                (false, false) => false,
                (true, true) => (20.0 * (h / r).log10()).abs() > LEVEL_TOLERANCE_DB,
                _ => true,
            };
            if differs {
                failures.push(format!(
                    "{when}: {name} on channel {channel} peaks {h:.4}, fresh build {r:.4} \
                     ({:+.1} dB)",
                    20.0 * (h.max(SILENT) / r.max(SILENT)).log10()
                ));
            }
        }
    }
}

/// The footswitch alone, K times. With the loop on, both heads are one
/// pipeline in one runtime. With it off, each guitar is its own runtime
/// (#703). Either way each guitar is processed by exactly ONE live runtime. A
/// second one plays it twice, which is the "several streams stacked" the owner
/// hears. The live graph must be exactly what a fresh build of that state is,
/// with no runtime or ring shared between two groups.
#[test]
fn k_insert_toggles_leave_every_head_in_exactly_one_live_runtime() {
    let mut rig = Rig::start(SYN2);
    for toggle in 1..=TOGGLES {
        let next = Scene {
            insert: !rig.scene.insert,
            ..rig.scene
        };
        rig.switch(next);
        rig.run(Play::Both, SETTLE);
        let live = rig.live();
        for head in [0usize, 1] {
            let owners: Vec<usize> = live
                .iter()
                .filter(|(_, runtime)| pipelines(runtime).iter().any(|(input, _)| input == &[head]))
                .map(|(group, _)| *group)
                .collect();
            assert_eq!(
                owners.len(),
                1,
                "toggle {toggle} (insert {}): the guitar on In {} is processed by the \
                 runtimes of groups {owners:?}",
                next.insert,
                head + 1
            );
        }
        assert_eq!(
            layout(&live),
            layout(&fresh_live(next)),
            "toggle {toggle} (insert {}): the live graph (left) is not a fresh build of \
             the same state (right)",
            next.insert
        );
        let shared = shared_state(&live);
        assert!(
            shared.is_empty(),
            "toggle {toggle} (insert {}): N streams = N isolated pipelines, yet {shared:?}",
            next.insert
        );
    }
}

/// Measured live: 5 routes with the loop on (`[0,1]` x2, `[10,11]` x2, `[3]`)
/// and 4 with it off. The loop-on five were the return written once per E/S
/// on the same physical outputs (#979); the return now writes each physical
/// output once, so the loop on is 3 routes (`[0,1]`, `[10,11]`, `[3]`) and the
/// loop off 4. After each of K switches the live runtimes own exactly that
/// many routes, and no route has two owning runtimes. Every runtime owns
/// exactly the routes its own pipelines write (#947). Its stream pops a route
/// that no pipeline writes empty on every callback: underruns in whole
/// 64-frame buffers that never stop.
#[test]
fn k_scene_switches_keep_three_routes_with_the_insert_on_and_four_with_it_off() {
    let mut rig = Rig::start(SYN2);
    for (k, &next) in SCENE_SWITCHES.iter().enumerate() {
        rig.switch(next);
        rig.run(Play::Both, SETTLE);
        let live = rig.live();
        let mut owners: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (group, runtime) in &live {
            for (route, _) in written_routes(runtime) {
                owners.entry(route).or_default().push(*group);
            }
        }
        let expected = if next.insert { ROUTES - 2 } else { ROUTES - 1 };
        assert!(
            owners.len() == expected && owners.values().all(|groups| groups.len() == 1),
            "switch {} -> {next:?}: expected {expected} routes with one owning runtime each; \
             route -> owning groups: {owners:?}",
            k + 1
        );
        for (group, runtime) in &live {
            let owned: BTreeSet<usize> = written_routes(runtime)
                .into_iter()
                .map(|(route, _)| route)
                .collect();
            let written: BTreeSet<usize> = pipelines(runtime)
                .into_iter()
                .flat_map(|(_, routes)| routes)
                .collect();
            assert_eq!(
                owned,
                written,
                "switch {} -> {next:?}: runtime {group} owns (left) other routes than its \
                 pipelines write (right)",
                k + 1
            );
        }
    }
}

/// No stale writer. After each switch, every route is written by exactly the
/// pipelines a fresh build of that scene writes it with, and by none from the
/// scene before: not the return still on the tails with the loop off, not a
/// head still on the send, not a head on the other head's outputs. Measured
/// live: two routes of the same stream degraded at very different rates
/// (12032 vs 2496 underrun frames).
#[test]
fn no_route_keeps_a_writer_of_the_scene_before() {
    let mut rig = Rig::start(SYN2);
    for (k, &next) in SCENE_SWITCHES.iter().enumerate() {
        rig.switch(next);
        rig.run(Play::Both, SETTLE);
        assert_eq!(
            writers(&rig.live()),
            writers(&fresh_live(next)),
            "switch {} -> {next:?}: route -> input channels of its writers, after the \
             switch (left) and in a fresh build (right)",
            k + 1
        );
    }
}

/// Route `[3]` is the insert send (the binding's `pedais` endpoint). While the
/// loop is off, no runtime owns it and channel 3 carries nothing, even with
/// both guitars playing. When a switch brings the loop back, exactly one route
/// feeds the pedals. In the live watch, route `[3]` appeared and disappeared
/// with the rebuilds.
#[test]
fn the_send_on_channel_3_exists_only_while_the_insert_is_on() {
    let mut rig = Rig::start(SYN2);
    for (k, &next) in SCENE_SWITCHES.iter().enumerate() {
        rig.switch(next);
        let sends = send_routes(&rig.live());
        let peak = rig.peaks(Play::Both)[3];
        if next.insert {
            assert!(
                sends.len() == 1 && peak > SILENT,
                "switch {} -> {next:?}: loop on, so one route must feed the pedals; \
                 (group, route) on [3]: {sends:?}, channel 3 peak {peak}",
                k + 1
            );
        } else {
            assert!(
                sends.is_empty() && peak < SILENT,
                "switch {} -> {next:?}: loop off, yet (group, route) on [3]: {sends:?}, \
                 channel 3 peak {peak}",
                k + 1
            );
        }
    }
}

/// Latency never grows, and a switch does not start a stream of dropouts.
/// Live rebuilds reuse routes (#670, #969), so whatever a switch leaves in a
/// ring stays there. After each switch every route is judged over four
/// drift-guard windows, with the worker always on time. The reference is a
/// fresh build of the same scene over its first four windows, which is the
/// owner's off/on. Compared with it, the switched rig may not queue more
/// frames (latency), underrun more frames or be trimmed more often. Measured
/// live: `latency_trims` rising and underruns climbing in 64-frame multiples
/// until the chain was rebuilt.
#[test]
fn a_switch_never_leaves_a_route_more_fill_trims_or_underruns_than_a_fresh_start() {
    let mut rig = Rig::start(SYN2);
    rig.health(POST);
    let mut failures = Vec::new();
    for (k, &next) in SCENE_SWITCHES.iter().enumerate() {
        rig.switch(next);
        let after = rig.health(POST);
        let fresh = Rig::start(next).health(POST);
        for route in worse_than(&after, &fresh) {
            failures.push(format!(
                "switch {} -> {next:?}, {route} (after the switch vs fresh)",
                k + 1
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} route(s) came out of a switch worse than a fresh build:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Volume per stream is immutable (#10). Whichever path a switch takes, each
/// guitar reaches each physical output at the level a fresh build of that
/// scene plays it at. It is never played twice (+6 dB: a head in two runtimes,
/// or a leftover route on the same output), never missing, and nothing sounds
/// where a fresh build is silent. Measured live: an output peak of +10.1 dBFS.
#[test]
fn after_each_switch_every_output_plays_each_guitar_at_its_fresh_level() {
    let mut failures = Vec::new();
    let mut rig = Rig::start(SYN2);
    for (k, &next) in SCENE_SWITCHES.iter().enumerate() {
        rig.switch(next);
        compare_levels(
            &mut rig,
            &format!("switch {} -> {next:?}", k + 1),
            &mut failures,
        );
    }
    for scenes in [LOOP_ON_SCENES, LOOP_OFF_SCENES] {
        let mut rig = Rig::start(scenes[0]);
        for k in 1..=IN_PLACE_SWITCHES {
            let next = scenes[k % scenes.len()];
            rig.switch_in_place(next);
            compare_levels(
                &mut rig,
                &format!("in-place switch {k} -> {next:?}"),
                &mut failures,
            );
        }
    }
    assert!(
        failures.is_empty(),
        "{} level(s) differ from a fresh build of the same scene:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A chain holding a VST3 switches scenes in place (#779), runtime by
/// runtime. With the insert where it was, nothing regroups. After each update
/// every runtime must be what a fresh build of the new scene is. Both guitars
/// come in on the HD 8's one input stream, so a runtime that kept or gained
/// the other guitar's pipeline plays that guitar twice. The update itself must
/// leave every ring exactly as it was (#592/#670: no re-prime, no reset, no
/// leftover pushed). Over K switches the routes may not pay anything that a
/// rig which never switched does not also pay.
#[test]
fn in_place_scene_changes_keep_each_runtime_its_head_its_routes_and_their_fill() {
    for scenes in [LOOP_ON_SCENES, LOOP_OFF_SCENES] {
        let mut rig = Rig::start(scenes[0]);
        let mut still = Rig::start(scenes[0]);
        let (mut switched, mut steady) = (BTreeMap::new(), BTreeMap::new());
        accumulate(&mut switched, rig.health(BETWEEN));
        accumulate(&mut steady, still.health(BETWEEN));
        for k in 1..=IN_PLACE_SWITCHES {
            let next = scenes[k % scenes.len()];
            let before = ring_fill(&rig.live());
            rig.switch_in_place(next);
            assert_eq!(
                ring_fill(&rig.live()),
                before,
                "in-place switch {k} -> {next:?}: (group, route, frames queued) changed \
                 across the update itself"
            );
            accumulate(&mut switched, rig.health(BETWEEN));
            accumulate(&mut steady, still.health(BETWEEN));
            assert_eq!(
                layout(&rig.live()),
                layout(&fresh_live(next)),
                "in-place switch {k} -> {next:?}: the updated runtimes (left) are not a \
                 fresh build of the scene (right)"
            );
        }
        let worse = worse_than(&switched, &steady);
        assert!(
            worse.is_empty(),
            "{IN_PLACE_SWITCHES} in-place switches (insert {}) cost the routes more than a \
             rig that never switched (switched vs still):\n{}",
            scenes[0].insert,
            worse.join("\n")
        );
    }
}

/// The owner's workaround, pinned as the reference the other tests use.
/// Switching the chain off takes every one of its runtimes out of the graph.
/// Switching it back on after K scene switches gives a fresh build.
#[test]
fn a_chain_switched_off_and_on_after_k_switches_is_a_fresh_build() {
    let mut rig = Rig::start(SYN2);
    for &next in &SCENE_SWITCHES {
        rig.switch(next);
        rig.run(Play::Both, SETTLE);
    }
    rig.graph.remove_chain(&chain_id());
    let left: Vec<usize> = rig.live().iter().map(|(group, _)| *group).collect();
    assert!(
        left.is_empty(),
        "the chain is off, yet the runtimes of groups {left:?} are still in the graph"
    );
    upsert(&mut rig.graph, rig.scene, true);
    rig.run(Play::Both, SETTLE);
    assert_eq!(
        layout(&rig.live()),
        layout(&fresh_live(rig.scene)),
        "switched back on (left) is not a fresh build (right)"
    );
}
