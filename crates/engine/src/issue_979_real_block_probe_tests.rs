//! #979 — hypothesis: the two readings the owner took from the offline probes
//! during the incident come from the REAL blocks, which every earlier harness
//! replaced with unity gain.
//!
//! - `peak_dbfs` read +10.1 dBFS (group 0, insert off). Two ways real blocks
//!   can cause that. The offline quality measurement may play the owner's NAM
//!   preamp and IR cab differently from how a head plays them live: other gain,
//!   another copy of the head, or the L and R of a mono head summed. Or a real
//!   block may carry state from one build to the next: a model or convolver
//!   shared by the probe, the live pipelines and the two heads. A shared block
//!   is also "several streams on top of each other".
//! - `dsp_latency_ms` read 8.48 ms, then 15.62 ms (x1.84). A block cannot
//!   report a latency to that probe, because `dsp_latency_ms` is the measured
//!   DSP time of one buffer (#334). So the stand-in for "a block that reads
//!   8.48 ms" is the real NAM and IR themselves, and what is measured is their
//!   DSP time per buffer. After the owner's scene switches (footswitches), the
//!   rig must do the DSP work of a fresh build of the scene it is on. A
//!   pipeline that a switch left behind and that keeps running costs its DSP
//!   time a second time, and plays a second time.
//!
//! The blocks are repo fixtures from `crates/engine/tests/fixtures/plugins`,
//! stored in git LFS:
//! - `nam_marshall_jmp1`, preset `clean`: an A2 preamp capture, the same
//!   architecture as the owner's `nam_synergy_dumble_os_a2` preamp.
//! - `ir_marshall_4x12_v30`, preset `b`, at its manifest `output_gain_db`
//!   (-18.31 dB). That is how the owner's presets save a cab.
//!
//! They replace the parent's `preamp` and `cab` stand-ins in `rig:input-7`.
//! No machine path is read. If LFS was not pulled, each test prints BLOCKED
//! and returns.
//!
//! The offline quality measurement is the engine's
//! `chain_quality::measure_quality`, run at the rig's 44.1 kHz and 64 frames.
//! Its battery sine is 1 kHz at half scale, pinned by
//! `feature_dsp::quality_metrics_tests::battery_sine_is_1khz_at_half_scale`,
//! so the live side plays the same sine.
//!
//! Child module of `issue_979_two_head_rig_tests`: the owner's topology comes
//! from there through `super::`. The HAL cycle is the parent's: every route's
//! output callback, then the #670 worker, on time. The worker's
//! `process_input_f32` is the DSP work that the latency reading measures.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::f64::consts::TAU;
use std::path::PathBuf;
use std::sync::{Arc, Once};
use std::time::Instant;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::{
    pipelines, rig_chain, rig_registry, written_routes, FRAMES, HD8_CHANNELS, RATE, ROUTES, TARGET,
};
use crate::chain_quality::measure_quality;
use crate::offline::render_chain;
use crate::runtime::{process_input_f32, process_output_f32, RuntimeGraph};
use crate::runtime_graph::build_per_input_runtimes;
use crate::runtime_state::{ChainRuntimeState, SPILLOVER_FRAMES};

const TWO_HEADS: [&str; 2] = ["guitarra-1", "guitarra-2"];

/// The A2 preamp capture that stands in for the owner's dumble A2.
const AMP_MODEL: &str = "nam_marshall_jmp1";
const AMP_PRESET: &str = "clean";
const AMP_CAPTURE: &str = "nam/marshall_jmp1/captures/jmp_clean.nam";
/// The cab IR, at the manifest's `output_gain_db` for preset `b`.
const CAB_MODEL: &str = "ir_marshall_4x12_v30";
const CAB_PRESET: &str = "b";
const CAB_OUTPUT_DB: f32 = -18.309_72;
const CAB_CAPTURE: &str = "ir/marshall_4x12_v30/ir/ev_mix_b.wav";

/// The quality battery's sine: 1 kHz at half scale (-6.02 dBFS).
const BATTERY_HZ: f64 = 1_000.0;
const BATTERY_AMP: f32 = 0.5;

/// A probe reading more than this away from what a route plays live is
/// reading something the rig does not play. Doubling one head is +6.02 dB.
const LEVEL_TOL_DB: f64 = 0.5;
/// Live seconds played for a level, and the last stretch counted as steady.
const LIVE_SECONDS: f32 = 2.0;
const STEADY_SECONDS: f32 = 0.5;
/// Largest sample difference on a head's route caused by the other head
/// playing (-120 dBFS). Isolated pipelines differ by exactly zero.
const ISOLATION_TOL: f32 = 1e-6;

/// #953 guard window: 8192 frames at 64 frames per callback.
const WINDOW: usize = 8_192 / FRAMES;
/// Cycles of playing between two footswitches (~93 ms).
const PLAY: usize = 64;
/// Cycles over which the DSP time is measured, per rig.
const MEASURE: usize = 1_024;
/// Cycles over which the levels after the switches are read (~370 ms).
const LEVEL_CYCLES: usize = 256;
/// Allowed growth of a group's median DSP time over a fresh build of the
/// same scene. The rig measured x1.84 (8.48 -> 15.62 ms).
const GROWTH: f64 = 1.5;
/// Timer slack on top of `GROWTH` (ns).
const SLACK_NANOS: f64 = 5_000.0;
/// -60 dBFS: a route above this plays.
const AUDIBLE: f32 = 1e-3;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins")
}

fn init_real_blocks() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        plugin_loader::registry::init(&fixtures_root());
    });
}

/// True when the repo's NAM and IR fixtures are real files, not git-lfs
/// pointers. Prints BLOCKED otherwise.
fn real_blocks_ready(test: &str) -> bool {
    for relative in [AMP_CAPTURE, CAB_CAPTURE] {
        let path = fixtures_root().join(relative);
        let head: Vec<u8> = std::fs::read(&path)
            .map(|bytes| bytes.into_iter().take(64).collect())
            .unwrap_or_default();
        if head.is_empty() || head.starts_with(b"version https://git-lfs") {
            eprintln!(
                "BLOCKED {test}: the repo fixture {} is missing or still a git-lfs pointer \
                 (run `git lfs pull`), so the real NAM and IR cannot be built",
                path.display()
            );
            return false;
        }
    }
    init_real_blocks();
    true
}

/// A disk-package block of `effect_type`/`model`: the schema's defaults, then
/// `choices` and `floats`, the way a saved preset carries them.
fn package_block(
    id: &str,
    effect_type: &str,
    model: &str,
    choices: &[(&str, &str)],
    floats: &[(&str, f32)],
) -> AudioBlock {
    let mut params = ParameterSet::default();
    for (path, value) in choices {
        params.insert(*path, ParameterValue::String((*value).into()));
    }
    for (path, value) in floats {
        params.insert(*path, ParameterValue::Float(*value));
    }
    let params = match schema_for_block_model(effect_type, model)
        .into_iter()
        .next()
    {
        Some(schema) => params
            .normalized_against(&schema)
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                panic!("setup: the {effect_type}/{model} params do not fit its schema")
            }),
        None => params,
    };
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.into(),
            model: model.into(),
            params,
        }),
    }
}

fn real_preamp() -> AudioBlock {
    package_block(
        "preamp",
        "preamp",
        AMP_MODEL,
        &[("preset", AMP_PRESET)],
        &[],
    )
}

fn real_cab() -> AudioBlock {
    package_block(
        "cab",
        block_core::EFFECT_TYPE_CAB,
        CAB_MODEL,
        &[("preset", CAB_PRESET)],
        &[("output_db", CAB_OUTPUT_DB)],
    )
}

/// One of the owner's scenes on `rig:input-7`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Scene {
    /// The SYN-2 loop.
    insert: bool,
    /// The NAM preamp.
    preamp: bool,
}

/// `ANAL+DIG`, the live capture's scene: insert off, NAM on.
const DIG: Scene = Scene {
    insert: false,
    preamp: true,
};
/// The same scene with the NAM switched off by footswitch.
const DIG_DRY: Scene = Scene {
    insert: false,
    preamp: false,
};
/// The SYN-2 in the loop, NAM on. Flipping the insert regroups the heads.
const LOOP: Scene = Scene {
    insert: true,
    preamp: true,
};
/// Eight footswitches that end on `DIG`: four keep the grouping, four flip it.
const SWITCHES: [Scene; 8] = [DIG_DRY, DIG, LOOP, DIG, DIG_DRY, DIG, LOOP, DIG];

/// `rig:input-7` on `heads`, in `scene`, with the real NAM preamp and IR cab
/// in place of the parent's stand-ins.
fn real_rig(heads: &[&str], scene: Scene) -> Chain {
    let mut chain = rig_chain(scene.insert);
    chain.io_binding_ids = heads.iter().map(|head| head.to_string()).collect();
    let mut replaced = 0;
    for block in chain.blocks.iter_mut() {
        match block.id.0.as_str() {
            "preamp" => {
                *block = real_preamp();
                block.enabled = scene.preamp;
                replaced += 1;
            }
            "cab" => {
                *block = real_cab();
                replaced += 1;
            }
            _ => {}
        }
    }
    assert_eq!(
        replaced, 2,
        "setup: the parent's rig:input-7 must hold a `preamp` and a `cab` block"
    );
    chain
}

fn chain_id() -> ChainId {
    rig_chain(false).id
}

fn db(level: f32) -> f64 {
    20.0 * f64::from(level.max(1e-9)).log10()
}

fn cycles_for(seconds: f32) -> usize {
    (seconds * RATE / FRAMES as f32).ceil() as usize
}

/// One interleaved HD 8 input buffer at `cycle`. Each source is (channel, Hz,
/// amplitude), a sine whose phase runs on across cycles.
fn input_at(cycle: usize, sources: &[(usize, f64, f32)]) -> Vec<f32> {
    let mut buffer = vec![0.0_f32; FRAMES * HD8_CHANNELS];
    for (i, frame) in buffer.chunks_mut(HD8_CHANNELS).enumerate() {
        let n = (cycle * FRAMES + i) as f64;
        for &(channel, hz, amplitude) in sources {
            frame[channel] = amplitude * (TAU * hz * n / f64::from(RATE)).sin() as f32;
        }
    }
    buffer
}

/// The battery sine on both guitars.
fn battery_on_both_heads(cycle: usize) -> Vec<f32> {
    input_at(
        cycle,
        &[(0, BATTERY_HZ, BATTERY_AMP), (1, BATTERY_HZ, BATTERY_AMP)],
    )
}

/// The owner playing through the scenes: a guitar on each head, and the
/// SYN-2 return carrying the pedals (live only with the loop on).
fn owner_playing(cycle: usize) -> Vec<f32> {
    input_at(
        cycle,
        &[
            (0, 330.0, 0.25),
            (1, 440.0, 0.25),
            (2, 550.0, 0.25),
            (3, 550.0, 0.25),
        ],
    )
}

/// The quality numbers the probe reports, as f64.
#[derive(Clone, Copy, Debug)]
struct Reading {
    peak_dbfs: f64,
    noise_floor_dbfs: f64,
    thd_n: f64,
    clip_fraction: f64,
}

/// The offline quality measurement of `chain` at the rig's rate and buffer.
fn probe(chain: &Chain) -> Reading {
    let m = measure_quality(chain, RATE, FRAMES)
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            panic!(
                "the quality probe must measure {} ({:?})",
                chain.id.0, chain.io_binding_ids
            )
        });
    Reading {
        peak_dbfs: f64::from(m.peak_dbfs),
        noise_floor_dbfs: f64::from(m.noise_floor_dbfs),
        thd_n: f64::from(m.thd_n),
        clip_fraction: f64::from(m.clip_fraction),
    }
}

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan()) || (a - b).abs() <= tolerance
}

/// Where `got` is not `want`. Two runs of a deterministic render agree far
/// inside these bounds. A block carrying state from another build does not.
fn differences(want: &Reading, got: &Reading) -> Vec<String> {
    let mut found = Vec::new();
    if !close(want.peak_dbfs, got.peak_dbfs, 0.01) {
        found.push(format!(
            "peak {:+.3} dBFS instead of {:+.3}",
            got.peak_dbfs, want.peak_dbfs
        ));
    }
    let both_below_24_bit = want.noise_floor_dbfs <= -120.0 && got.noise_floor_dbfs <= -120.0;
    if !both_below_24_bit && !close(want.noise_floor_dbfs, got.noise_floor_dbfs, 0.01) {
        found.push(format!(
            "noise floor {:+.3} dBFS instead of {:+.3}",
            got.noise_floor_dbfs, want.noise_floor_dbfs
        ));
    }
    let thd_tolerance = 1e-6 + 1e-4 * want.thd_n.abs().max(got.thd_n.abs());
    if !close(want.thd_n, got.thd_n, thd_tolerance) {
        found.push(format!(
            "THD+N {:.6} instead of {:.6}",
            got.thd_n, want.thd_n
        ));
    }
    if !close(want.clip_fraction, got.clip_fraction, 1e-9) {
        found.push(format!(
            "clip fraction {:.6} instead of {:.6}",
            got.clip_fraction, want.clip_fraction
        ));
    }
    found
}

/// Setup guard: the real NAM and IR build (nothing faulted into a bypass) and
/// change the signal. Otherwise these tests would measure unity gain again.
fn assert_real_blocks_run(scene: Scene) {
    let di: Vec<[f32; 2]> = (0..cycles_for(0.25) * FRAMES)
        .map(|n| {
            let s = BATTERY_AMP * (TAU * BATTERY_HZ * n as f64 / f64::from(RATE)).sin() as f32;
            [s, s]
        })
        .collect();
    let chain = real_rig(&TWO_HEADS[..1], scene);
    let outcome = render_chain(&chain, RATE, &di, FRAMES, 0)
        .unwrap_or_else(|e| panic!("setup: the real-block rig must render offline: {e:?}"));
    assert!(
        outcome.faulted_blocks.is_empty(),
        "setup: the repo fixtures {AMP_MODEL}/{CAB_MODEL} did not build and were bypassed, \
         so this would measure unity gain again: {:?}",
        outcome.faulted_blocks
    );
    let changed = outcome
        .samples
        .iter()
        .zip(&di)
        .map(|(out, dry)| (out[0] - dry[0]).abs().max((out[1] - dry[1]).abs()))
        .fold(0.0_f32, f32::max);
    assert!(
        changed > 1e-3,
        "setup: the real NAM and IR left the signal unchanged (largest difference \
         {changed}), so they are not processing"
    );
}

/// The chain's live runtimes, one per group, built as the rig builds them.
fn live_runtimes(chain: &Chain) -> Vec<Arc<ChainRuntimeState>> {
    build_per_input_runtimes(
        chain,
        RATE,
        &HashMap::new(),
        &[TARGET; ROUTES],
        &rig_registry(),
    )
    .expect("the owner's chain with the real blocks must build")
    .into_iter()
    .map(|(_, state)| Arc::new(state))
    .collect()
}

/// What one route played in one callback, summed over every runtime that
/// owns it (a second owner is a doubled stream).
struct RouteOut {
    channels: Vec<usize>,
    buffer: Vec<f32>,
}

impl RouteOut {
    fn peak(&self) -> f32 {
        self.buffer
            .chunks(HD8_CHANNELS)
            .flat_map(|frame| self.channels.iter().map(move |&ch| frame[ch].abs()))
            .fold(0.0, f32::max)
    }
}

/// One HAL cycle: what each route played, and the wall time (ns) of each
/// runtime's worker DSP (`process_input_f32`) in `runtimes` order.
struct Cycle {
    routes: BTreeMap<usize, RouteOut>,
    dsp_nanos: Vec<u128>,
}

fn hal_cycle(runtimes: &[Arc<ChainRuntimeState>], input: &[f32]) -> Cycle {
    let mut routes: BTreeMap<usize, RouteOut> = BTreeMap::new();
    let mut stream = vec![0.0_f32; FRAMES * HD8_CHANNELS];
    for runtime in runtimes {
        for (route, channels) in written_routes(runtime) {
            stream.fill(0.0);
            process_output_f32(runtime, route, &mut stream, HD8_CHANNELS);
            let entry = routes.entry(route).or_insert_with(|| RouteOut {
                channels,
                buffer: vec![0.0; FRAMES * HD8_CHANNELS],
            });
            for (sum, sample) in entry.buffer.iter_mut().zip(&stream) {
                *sum += *sample;
            }
        }
    }
    let mut dsp_nanos = Vec::with_capacity(runtimes.len());
    for runtime in runtimes {
        let start = Instant::now();
        process_input_f32(runtime, 0, input, HD8_CHANNELS);
        dsp_nanos.push(start.elapsed().as_nanos());
    }
    Cycle { routes, dsp_nanos }
}

fn median(values: &[u128]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2] as f64
}

/// The median DSP time (ns) of each group.
fn group_medians(per_group: &[Vec<u128>]) -> Vec<f64> {
    per_group.iter().map(|samples| median(samples)).collect()
}

/// `rig:input-7` with the real blocks, live in a runtime graph, switched by
/// the bank/scene navigator (`upsert_chain_spillover`, #454), which is the
/// path every footswitch takes.
struct SceneRig {
    graph: RuntimeGraph,
    scene: Scene,
    clock: usize,
}

impl SceneRig {
    fn start(scene: Scene) -> Self {
        let mut graph = RuntimeGraph {
            chains: HashMap::new(),
        };
        graph
            .upsert_chain(
                &real_rig(&TWO_HEADS, scene),
                RATE,
                &HashMap::new(),
                true,
                &[TARGET; ROUTES],
                &rig_registry(),
            )
            .expect("the owner's chain with the real blocks must build");
        Self {
            graph,
            scene,
            clock: 0,
        }
    }

    fn switch(&mut self, next: Scene) {
        let structural = next.insert != self.scene.insert;
        self.graph
            .upsert_chain_spillover(
                &real_rig(&TWO_HEADS, next),
                RATE,
                &HashMap::new(),
                structural,
                &[TARGET; ROUTES],
                &rig_registry(),
            )
            .expect("the navigator's switch must apply");
        self.scene = next;
    }

    /// The chain's live runtimes, by group.
    fn live(&self) -> Vec<Arc<ChainRuntimeState>> {
        let mut live = self.graph.runtimes_with_groups_for(&chain_id());
        live.sort_by_key(|(group, _)| *group);
        live.into_iter().map(|(_, runtime)| runtime).collect()
    }

    fn cycle(&mut self) -> Cycle {
        let input = owner_playing(self.clock);
        self.clock += 1;
        hal_cycle(&self.live(), &input)
    }

    fn play(&mut self, cycles: usize) {
        for _ in 0..cycles {
            self.cycle();
        }
    }

    /// Live runtimes, pipelines, pipelines still holding a spillover tail,
    /// and each runtime's own DSP latency reading (ms).
    fn shape(&self) -> String {
        let live = self.live();
        let pipelines_live: usize = live.iter().map(|runtime| pipelines(runtime).len()).sum();
        let outgoing: usize = live
            .iter()
            .map(|runtime| {
                runtime
                    .processing
                    .lock()
                    .unwrap()
                    .input_states
                    .iter()
                    .filter(|state| state.outgoing.is_some())
                    .count()
            })
            .sum();
        let readings: Vec<String> = live
            .iter()
            .map(|runtime| format!("{:.3}", runtime.measured_latency_ms()))
            .collect();
        format!(
            "{} runtime(s), {pipelines_live} pipeline(s), {outgoing} spillover tail(s), \
             measured_latency_ms {readings:?}",
            live.len()
        )
    }
}

/// Cycles after the last switch before anything is compared: past the
/// spillover window and two guard windows.
fn settle_cycles() -> usize {
    SPILLOVER_FRAMES.div_ceil(FRAMES) + 2 * WINDOW
}

/// The rig after the owner's eight footswitches, back on `DIG`, playing all
/// along, and a fresh build of `DIG` that played as long after its start.
fn switched_and_fresh() -> (SceneRig, SceneRig) {
    let mut switched = SceneRig::start(DIG);
    switched.play(PLAY);
    for next in SWITCHES {
        switched.switch(next);
        switched.play(PLAY);
    }
    assert_eq!(switched.scene, DIG, "setup: the switches end on DIG");
    switched.play(settle_cycles());
    let mut fresh = SceneRig::start(DIG);
    fresh.play(settle_cycles());
    (switched, fresh)
}

/// The probe measures one stream. Binding a second guitar to the chain must
/// not change the report of the owner's real preamp and cab: each head alone
/// reads what both heads together read, with the loop off (the live capture)
/// and on.
#[test]
fn a_real_nam_and_ir_rig_probes_the_same_with_two_heads_as_with_either_head_alone() {
    if !real_blocks_ready("two_heads_as_either_head_alone") {
        return;
    }
    assert_real_blocks_run(DIG);
    let mut failures = Vec::new();
    for scene in [DIG, LOOP] {
        let both = probe(&real_rig(&TWO_HEADS, scene));
        for head in TWO_HEADS {
            let alone = probe(&real_rig(&[head], scene));
            let diff = differences(&alone, &both);
            if !diff.is_empty() {
                failures.push(format!(
                    "{scene:?}: two heads read {} next to `{head}` alone ({alone:?} vs {both:?})",
                    diff.join(", ")
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "the quality probe of the real NAM + IR rig changed when a second guitar was bound \
         (the rig read +10.1 dBFS):\n{}",
        failures.join("\n")
    );
}

/// The probe must read the level the chain plays. Live, the owner's two-head
/// rig (insert off, the live capture's scene) plays the battery sine on both
/// guitars through the real preamp and cab at 44.1 kHz and 64 frames. Every
/// route plays one head. The probe's peak must lie between the route's
/// steady level and its loudest buffer, within 0.5 dB, on every route.
/// Stacked heads or pipelines read +6 dB or more. The rig's probe read
/// +10.1 dBFS.
#[test]
fn the_probe_peak_of_a_real_nam_and_ir_rig_is_the_level_each_route_plays_live() {
    if !real_blocks_ready("probe_peak_is_the_live_level") {
        return;
    }
    assert_real_blocks_run(DIG);
    let chain = real_rig(&TWO_HEADS, DIG);
    let probed = probe(&chain).peak_dbfs;

    let runtimes = live_runtimes(&chain);
    let cycles = cycles_for(LIVE_SECONDS);
    let steady_from = cycles - cycles_for(STEADY_SECONDS);
    let mut loudest: BTreeMap<usize, f32> = BTreeMap::new();
    let mut steady: BTreeMap<usize, f32> = BTreeMap::new();
    let mut channels: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for cycle in 0..cycles {
        let heard = hal_cycle(&runtimes, &battery_on_both_heads(cycle));
        for (route, out) in heard.routes {
            let peak = out.peak();
            let entry = loudest.entry(route).or_default();
            *entry = entry.max(peak);
            if cycle >= steady_from {
                let entry = steady.entry(route).or_default();
                *entry = entry.max(peak);
            }
            channels.entry(route).or_insert(out.channels);
        }
    }
    assert_eq!(
        steady.len(),
        4,
        "setup: insert off, the two heads play Main and Out 2 each, 4 routes: {channels:?}"
    );
    assert!(
        steady.values().all(|level| *level > AUDIBLE),
        "setup: every route must play the battery sine live: {steady:?}"
    );

    let mut failures = Vec::new();
    for (route, level) in &steady {
        let (steady_db, loudest_db) = (db(*level), db(loudest[route]));
        if probed > loudest_db + LEVEL_TOL_DB || probed < steady_db - LEVEL_TOL_DB {
            failures.push(format!(
                "route {route} {:?}: plays {steady_db:+.2} dBFS steady, {loudest_db:+.2} dBFS \
                 at its loudest; the probe read {probed:+.2} dBFS ({:+.2} dB)",
                channels[route],
                probed - steady_db
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "the quality probe of the real NAM + IR rig does not read what a head plays live \
         (the rig's probe read +10.1 dBFS):\n{}",
        failures.join("\n")
    );
}

/// The offline render is deterministic (invariant #9). With the real blocks
/// the report must be the same on every call: again right away, while the
/// same chain plays live on its own worker thread (the owner probed a live
/// rig), after probing other scenes of the chain, and after the live rig is
/// gone. A model or convolver that one build hands to the next carries the
/// last signal it saw into the next report.
#[test]
fn the_probe_of_a_real_nam_and_ir_rig_reads_the_same_on_every_call_while_it_plays() {
    if !real_blocks_ready("probe_reads_the_same_on_every_call") {
        return;
    }
    assert_real_blocks_run(DIG);
    let chain = real_rig(&TWO_HEADS, DIG);
    let first = probe(&chain);
    let mut failures = Vec::new();
    let mut check = |label: &str, reading: Reading| {
        let diff = differences(&first, &reading);
        if !diff.is_empty() {
            failures.push(format!("{label}: {}", diff.join(", ")));
        }
    };

    check("called again", probe(&chain));

    let live = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let runtimes = live_runtimes(&chain);
                for cycle in 0..cycles_for(1.0) {
                    let loud = input_at(cycle, &[(0, 220.0, 0.9), (1, 147.0, 0.9)]);
                    hal_cycle(&runtimes, &loud);
                }
                runtimes
            })
            .join()
            .expect("the live worker thread must not panic")
    });
    check("while the chain is live", probe(&chain));

    probe(&real_rig(&TWO_HEADS, LOOP));
    probe(&real_rig(&TWO_HEADS, DIG_DRY));
    check("after probing the LOOP and DIG_DRY scenes", probe(&chain));

    drop(live);
    check("after the live chain was dropped", probe(&chain));

    assert!(
        failures.is_empty(),
        "the quality probe of the unchanged real NAM + IR chain read differently from its \
         first call {first:?}:\n{}",
        failures.join("\n")
    );
}

/// N streams = N isolated pipelines, with the real blocks. Insert off (the
/// live capture), guitar 1 plays the battery sine. Guitar 1's routes must
/// play the same samples whether guitar 2 is silent or plays a loud note. A
/// NAM or IR that the two heads share would put guitar 2 into guitar 1's
/// routes: "several streams on top of each other".
#[test]
fn a_real_nam_and_ir_head_plays_the_same_samples_whether_or_not_the_other_head_plays() {
    if !real_blocks_ready("head_isolation_with_real_blocks") {
        return;
    }
    assert_real_blocks_run(DIG);
    let chain = real_rig(&TWO_HEADS, DIG);
    let alone = live_runtimes(&chain);
    let together = live_runtimes(&chain);
    let guitar_1_routes: BTreeSet<usize> = alone
        .iter()
        .flat_map(|runtime| pipelines(runtime))
        .filter(|(input, _)| input == &vec![0])
        .flat_map(|(_, routes)| routes)
        .collect();
    assert!(
        !guitar_1_routes.is_empty(),
        "setup: guitar 1 (In 1) must write routes"
    );

    let guitar_1 = (0, BATTERY_HZ, BATTERY_AMP);
    let mut worst: Option<(usize, usize, f32)> = None;
    let mut guitar_2_loudest = 0.0_f32;
    for cycle in 0..cycles_for(1.0) {
        let a = hal_cycle(&alone, &input_at(cycle, &[guitar_1]));
        let b = hal_cycle(&together, &input_at(cycle, &[guitar_1, (1, 147.0, 0.9)]));
        for route in &guitar_1_routes {
            let (Some(silent_2), Some(playing_2)) = (a.routes.get(route), b.routes.get(route))
            else {
                continue;
            };
            let diff = silent_2
                .buffer
                .iter()
                .zip(&playing_2.buffer)
                .map(|(x, y)| (x - y).abs())
                .fold(0.0_f32, f32::max);
            if diff > worst.map_or(ISOLATION_TOL, |(_, _, d)| d) {
                worst = Some((cycle, *route, diff));
            }
        }
        for (route, out) in &b.routes {
            if !guitar_1_routes.contains(route) {
                guitar_2_loudest = guitar_2_loudest.max(out.peak());
            }
        }
    }
    assert!(
        guitar_2_loudest > AUDIBLE,
        "setup: guitar 2's routes must play its note ({guitar_2_loudest})"
    );
    assert_eq!(
        worst, None,
        "(cycle, route, largest sample difference): guitar 1's routes {guitar_1_routes:?} \
         changed when guitar 2 played through the real NAM + IR, so the two heads are not \
         isolated pipelines"
    );
}

/// Latency never grows. The owner switched scenes by footswitch and the DSP
/// latency went from 8.48 to 15.62 ms. Here the real NAM + IR rig takes eight
/// navigator switches, back on `DIG`, while the guitars play. Once the
/// spillover window has passed, each runtime's worker DSP time per buffer
/// must be what a fresh build of `DIG` costs: the median over 1024 buffers
/// within x1.5 (+5 us). Buffers of the two rigs are timed alternately, so
/// machine load hits both the same. A pipeline left behind by a switch that
/// keeps running doubles the time (x1.84 measured).
#[test]
fn after_scene_switches_a_real_nam_and_ir_rig_costs_the_dsp_time_of_a_fresh_build() {
    if !real_blocks_ready("dsp_time_after_scene_switches") {
        return;
    }
    assert_real_blocks_run(DIG);
    let (mut switched, mut fresh) = switched_and_fresh();
    let mut times: [Vec<Vec<u128>>; 2] = [Vec::new(), Vec::new()];
    for i in 0..MEASURE {
        let order = if i % 2 == 0 { [0, 1] } else { [1, 0] };
        for which in order {
            let cycle = if which == 0 {
                switched.cycle()
            } else {
                fresh.cycle()
            };
            let per_group = &mut times[which];
            if per_group.len() < cycle.dsp_nanos.len() {
                per_group.resize(cycle.dsp_nanos.len(), Vec::new());
            }
            for (group, nanos) in cycle.dsp_nanos.into_iter().enumerate() {
                per_group[group].push(nanos);
            }
        }
    }
    let (after, reference) = (group_medians(&times[0]), group_medians(&times[1]));
    assert!(
        reference.iter().all(|nanos| *nanos > 1_000.0),
        "setup: a fresh DIG build with the real NAM + IR must cost measurable DSP time per \
         buffer, got medians (ns) {reference:?}"
    );

    let shapes = format!(
        "after the switches: {}; fresh build: {}",
        switched.shape(),
        fresh.shape()
    );
    assert_eq!(
        after.len(),
        reference.len(),
        "after eight footswitches back to DIG the chain runs {} runtime(s), a fresh DIG \
         build {} ({shapes})",
        after.len(),
        reference.len()
    );
    let grown: Vec<String> = after
        .iter()
        .zip(&reference)
        .enumerate()
        .filter(|(_, (a, r))| **a > GROWTH * **r + SLACK_NANOS)
        .map(|(group, (a, r))| {
            format!(
                "group {group}: {:.1} us per buffer after the switches, {:.1} us fresh (x{:.2})",
                a / 1_000.0,
                r / 1_000.0,
                a / r
            )
        })
        .collect();
    assert!(
        grown.is_empty(),
        "the real NAM + IR rig does more DSP work per buffer after the owner's scene \
         switches than a fresh build of the same scene (measured on the rig: 8.48 -> \
         15.62 ms, x1.84):\n{}\n{shapes}",
        grown.join("\n")
    );
}

/// Per-stream volume is immutable, and nothing is played twice. After the
/// same eight footswitches back on `DIG`, every route of the real NAM + IR
/// rig must play the guitars at the level a fresh `DIG` build plays them,
/// within 0.5 dB, and the two rigs must play the same routes. A pipeline a
/// switch left behind still writing a route plays that guitar twice: +6 dB.
#[test]
fn after_scene_switches_each_route_of_a_real_nam_and_ir_rig_plays_its_fresh_level() {
    if !real_blocks_ready("route_levels_after_scene_switches") {
        return;
    }
    assert_real_blocks_run(DIG);
    let (mut switched, mut fresh) = switched_and_fresh();
    let mut levels: [BTreeMap<usize, f32>; 2] = [BTreeMap::new(), BTreeMap::new()];
    let mut channels: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for _ in 0..LEVEL_CYCLES {
        for (which, rig) in [&mut switched, &mut fresh].into_iter().enumerate() {
            for (route, out) in rig.cycle().routes {
                let entry = levels[which].entry(route).or_default();
                *entry = entry.max(out.peak());
                channels.entry(route).or_insert(out.channels);
            }
        }
    }
    let [after, reference] = &levels;
    assert!(
        !reference.is_empty() && reference.values().all(|level| *level > AUDIBLE),
        "setup: every route of a fresh DIG build must play the guitars: {reference:?}"
    );

    let routes: BTreeSet<usize> = after.keys().chain(reference.keys()).copied().collect();
    let mut failures = Vec::new();
    for route in routes {
        match (after.get(&route), reference.get(&route)) {
            (Some(a), Some(r)) => {
                let delta = db(*a) - db(*r);
                if delta.abs() > LEVEL_TOL_DB {
                    failures.push(format!(
                        "route {route} {:?}: {:+.2} dBFS after the switches, {:+.2} dBFS in a \
                         fresh build ({delta:+.2} dB)",
                        channels[&route],
                        db(*a),
                        db(*r)
                    ));
                }
            }
            (a, r) => failures.push(format!(
                "route {route} {:?}: played after the switches {a:?}, in a fresh build {r:?}",
                channels.get(&route)
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "after the owner's scene switches the real NAM + IR rig plays a level a fresh build \
         of the same scene does not ({}):\n{}",
        switched.shape(),
        failures.join("\n")
    );
}
