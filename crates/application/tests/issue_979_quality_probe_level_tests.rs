//! #979: the offline probes behind `openrig://chains/{chain}/quality` and
//! `openrig://chains/{chain}/latency`, run on the owner's two-head
//! `rig:input-7`.
//!
//! Hypothesis: the two readings the owner took from these probes during the
//! incident are artefacts of the probes, not what the rig plays.
//!
//! - The +10.1 dBFS `peak_dbfs` (group 0, insert off) comes from the probe
//!   playing the chain's heads, or its routes, on top of each other. A
//!   two-head chain would then read louder than one head plays live.
//! - `dsp_latency_ms` went from 8.48 to 15.62 between calls. Either state
//!   builds up in the probe path from one call to the next, or the probe runs
//!   at a different rate or buffer on different calls.
//!
//! Every block is a unity `gain:volume` (volume 100 %, not muted) standing in
//! for the NAM preamp and the IR cab, and the chain volume is 100 %. The
//! quality battery is a 1 kHz sine at half scale, -6.02 dBFS (pinned by
//! `feature_dsp::quality_metrics_tests::battery_sine_is_1khz_at_half_scale`).
//! A probe that reads one head at its live level therefore reads -6.02 dBFS.
//! Two heads stacked would read 0 dBFS, and four routes +6 dBFS.
//!
//! Every read goes through `application::read::resolve`, the resolver the MCP
//! resources answer with. The context carries the owner's E/S registry and the
//! HD 8 at 44.1 kHz with a 64-frame buffer.
//!
//! `dsp_latency_ms` is the elapsed time of one buffer on a fresh runtime
//! (#334), so it cannot be compared for equality. The latency tests pin what
//! must stay the same on every call (rate, buffer) and that the reading does
//! not grow.
//!
//! If these tests are green, the probes are ruled out. The +10.1 dBFS is then
//! the chain's own gain (preamp and cab), and the 8.48 to 15.62 ms change is
//! load on the machine, not state in the probe.
//!
//! Integration test of the `application` crate. It uses only the public read
//! API, so no production file registers it.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Once;

use application::bridge::QueryKind;
use application::live_source::LiveSource;
use application::local_dispatcher::LocalDispatcher;
use application::read::{resolve, ReadContext};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use engine::offline::render_chain;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;
use serde_json::Value;

const HD8: &str = "coreaudio:TUSBAudio:Fender:Quantum HD 8";
const CHAIN: &str = "rig:input-7";
/// The owner's interface rate.
const RATE: u32 = 44_100;
/// The owner's device buffer.
const FRAMES: u32 = 64;
const TWO_HEADS: [&str; 2] = ["guitarra-1", "guitarra-2"];
/// The quality battery's sine: half scale.
const BATTERY_PEAK_DBFS: f64 = -6.02;
/// A -6.02 dBFS sample.
const MINUS_6_DBFS: f32 = 0.5;
/// Level tolerance on a probe reading (dB). Stacking is +6 dB, so this is
/// far below any doubling.
const LEVEL_TOL_DB: f64 = 0.1;
/// Silent lead-in of the DI, so the render's start is not in the comparison.
const SETTLE: usize = 4_096;
/// Largest constant delay the offline render may add to a unity chain.
const MAX_LAG: usize = 256;
/// Sample tolerance of the unity render. A replayed or skipped buffer of the
/// DI is off by the signal itself (0.1 to 0.5).
const SAMPLE_TOL: f32 = 1e-3;
/// Latency probes per chain state and round of the growth test.
const PROBES_PER_ROUND: usize = 4;
/// Rounds of the growth test. Each round switches the insert off then on,
/// the way the owner's scenes do.
const ROUNDS: usize = 16;
/// Allowed growth of the median latency reading from the first quarter of
/// the calls to the last quarter. The measured 8.48 -> 15.62 ms is x1.84.
const GROWTH: f64 = 1.5;
/// Absolute slack (ms) on top of `GROWTH`, for timer noise on a reading that
/// is fractions of a millisecond on unity blocks.
const SLACK_MS: f64 = 0.1;

fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        engine::native_registry::register_all_natives();
    });
}

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

/// The owner's `config.yaml` bindings this chain selects (the same registry
/// as the engine's `issue_979_two_head_rig_tests::rig_registry`).
fn rig_registry() -> Vec<IoBinding> {
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

/// `gain:volume` at exactly unity: volume 100 %, not muted.
fn unity_volume() -> ParameterSet {
    init();
    let schema =
        schema_for_block_model("gain", "volume").expect("the volume block must have a schema");
    let mut params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("the volume defaults must normalize");
    params.insert("volume", ParameterValue::Float(100.0));
    params.insert("mute", ParameterValue::Bool(false));
    params
}

fn unity(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: unity_volume(),
        }),
    }
}

fn chain_id() -> ChainId {
    ChainId(CHAIN.into())
}

/// `rig:input-7` on the given E/S: the insert first, then the preamp and cab,
/// both at unity.
fn rig(heads: &[&str], insert_enabled: bool) -> Chain {
    Chain {
        id: chain_id(),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: heads.iter().map(|id| id.to_string()).collect(),
        blocks: vec![
            AudioBlock {
                id: BlockId("syn2-main".into()),
                enabled: insert_enabled,
                kind: AudioBlockKind::Insert(InsertBlock {
                    model: "standard".into(),
                    io: "syn2-main".into(),
                }),
            },
            unity("preamp"),
            unity("cab"),
        ],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

/// The HD 8's saved setting: 44.1 kHz, 64 frames.
fn hd8_settings() -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(HD8.into()),
        sample_rate: RATE,
        buffer_size_frames: FRAMES,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

/// The GUI's answer for this chain's rate: the HD 8 runs at 44.1 kHz.
struct Hd8;

impl LiveSource for Hd8 {
    fn chain_sample_rate(&self, _chain: &ChainId) -> Option<f32> {
        Some(RATE as f32)
    }
}

/// One project holding the chain, read the way the MCP resources read it.
/// The dispatcher is built once and reused, so repeated reads measure the
/// probe and not the harness.
struct Probe {
    project: Project,
    registry: Vec<IoBinding>,
    dispatcher: LocalDispatcher,
}

impl Probe {
    fn new(chain: Chain) -> Self {
        init();
        let project = Project {
            name: Some("issue-979".into()),
            device_settings: vec![hd8_settings()],
            chains: vec![chain],
            midi: None,
        };
        let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(project.clone())));
        Self {
            project,
            registry: rig_registry(),
            dispatcher,
        }
    }

    fn read(&self, kind: QueryKind) -> Value {
        let json = resolve(
            &kind,
            &ReadContext {
                project: &self.project,
                rig: None,
                io_bindings: self.registry.as_slice(),
                dispatcher: &self.dispatcher,
                live: &Hd8,
            },
        )
        .unwrap_or_else(|e| panic!("{kind:?} on {CHAIN} must answer: {e}"));
        serde_json::from_str(&json).unwrap_or_else(|e| {
            panic!("{kind:?} answered something that is not JSON ({e}): {json}")
        })
    }

    /// The `quality` object of `openrig://chains/{chain}/quality`.
    fn quality(&self) -> Value {
        let report = self.read(QueryKind::ChainQualityReport { chain: chain_id() });
        let quality = report["quality"].clone();
        assert!(
            quality.is_object(),
            "the quality report has no quality object: {report}"
        );
        quality
    }

    /// The whole `openrig://chains/{chain}/latency` answer.
    fn latency(&self) -> Value {
        self.read(QueryKind::ChainLatency { chain: chain_id() })
    }
}

fn number(v: &Value, key: &str) -> f64 {
    v[key]
        .as_f64()
        .unwrap_or_else(|| panic!("no numeric `{key}` in {v}"))
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("a latency reading is a number"));
    sorted[sorted.len() / 2]
}

fn dbfs(level: f32) -> f64 {
    20.0 * f64::from(level.max(1e-9)).log10()
}

/// A mono guitar DI at -6 dBFS: a silent lead-in of `SETTLE` frames, then
/// plucked-note stand-ins (decaying noise bursts, one every half second),
/// scaled so the loudest sample is exactly 0.5. Noise, not a sine, so a
/// replayed or skipped buffer cannot line up with the input.
fn di_minus_6_dbfs(seconds: f32) -> Vec<[f32; 2]> {
    let rate = RATE as f32;
    let frames = (seconds * rate) as usize;
    let note = (0.5 * rate) as usize;
    let mut seed: u32 = 0x0979_0001;
    let raw: Vec<f32> = (0..frames)
        .map(|n| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            if n < SETTLE {
                return 0.0;
            }
            let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            let age = ((n - SETTLE) % note) as f32 / rate;
            noise * (-age * 12.0).exp()
        })
        .collect();
    let loudest = raw.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    raw.iter()
        .map(|s| {
            let s = s * (MINUS_6_DBFS / loudest);
            [s, s]
        })
        .collect()
}

/// The constant delay (frames) at which `out` matches `di` best after the
/// lead-in, and the largest sample error at that delay.
fn best_alignment(di: &[[f32; 2]], out: &[[f32; 2]]) -> (usize, f32) {
    let mut best = (0usize, f32::INFINITY);
    for lag in 0..=MAX_LAG {
        let mut err = 0.0f32;
        for n in SETTLE..di.len().saturating_sub(MAX_LAG) {
            for c in 0..2 {
                err = err.max((out[n + lag][c] - di[n][c]).abs());
            }
            if err >= best.1 {
                break;
            }
        }
        if err < best.1 {
            best = (lag, err);
        }
    }
    best
}

/// The measured case: insert off, two heads, the battery sine at -6.02 dBFS
/// through unity blocks. The probe must read one head at the level it plays
/// live, -6.02 dBFS. It must not read the two heads stacked (0 dBFS), four
/// routes (+6 dBFS), or anything like the measured +10.1 dBFS.
#[test]
fn the_quality_probe_reads_one_head_at_its_live_level() {
    let q = Probe::new(rig(&TWO_HEADS, false)).quality();
    let peak = number(&q, "peak_dbfs");
    assert!(
        (peak - BATTERY_PEAK_DBFS).abs() <= LEVEL_TOL_DB,
        "insert off, two heads, unity blocks: the quality probe fed its -6.02 dBFS \
         battery sine read a {peak:+.2} dBFS peak, {:+.2} dB away from one head's live \
         level (0 dBFS = the two heads stacked, +6 dBFS = four routes; the rig's \
         probe read +10.1 dBFS): {q}",
        peak - BATTERY_PEAK_DBFS
    );
    assert_eq!(
        number(&q, "clip_fraction"),
        0.0,
        "a -6 dBFS sine through unity blocks must not clip in the probe: {q}"
    );
}

/// Same chain with the insert on. Offline there is no SYN-2 to feed the
/// return, so the insert cannot add level. The probe must still read one
/// head at -6.02 dBFS: not the send and return summed, not the return played
/// once per head E/S.
#[test]
fn the_quality_probe_reads_one_head_at_its_live_level_with_the_insert_on() {
    let q = Probe::new(rig(&TWO_HEADS, true)).quality();
    let peak = number(&q, "peak_dbfs");
    assert!(
        (peak - BATTERY_PEAK_DBFS).abs() <= LEVEL_TOL_DB,
        "insert on, two heads, unity blocks: the quality probe fed its -6.02 dBFS \
         battery sine read a {peak:+.2} dBFS peak, {:+.2} dB away from one head's \
         level: {q}",
        peak - BATTERY_PEAK_DBFS
    );
}

/// N streams = N isolated pipelines. The probe measures a stream, so binding
/// a second guitar to the chain must not change a single number in the
/// report. Each head alone reads exactly what both heads together read.
#[test]
fn the_quality_report_of_two_heads_is_the_report_of_each_head_alone() {
    let both = Probe::new(rig(&TWO_HEADS, false)).quality();
    for head in TWO_HEADS {
        let alone = Probe::new(rig(&[head], false)).quality();
        assert_eq!(
            both, alone,
            "insert off, unity blocks: the quality report changed when `{head}` got a \
             sibling head on the chain. Two heads {both}; `{head}` alone {alone}"
        );
    }
}

/// The report is an offline render of a fixed battery, so it is
/// deterministic (invariant #9). Probing again must give the same report,
/// and so must probing again after the latency probe has run many times. A
/// probe that keeps state between calls reads differently on every call.
#[test]
fn repeated_quality_probes_report_the_same_numbers() {
    let probe = Probe::new(rig(&TWO_HEADS, false));
    let first = probe.quality();
    for call in 2..=4 {
        let again = probe.quality();
        assert_eq!(
            again, first,
            "quality probe call {call} on the unchanged chain read {again}, the first read {first}"
        );
    }
    for _ in 0..16 {
        probe.latency();
    }
    let after = probe.quality();
    assert_eq!(
        after, first,
        "after 16 latency probes the quality probe read {after}, before them {first}"
    );
}

/// The render path the quality probe uses (`engine::offline::render_chain`),
/// fed a -6 dBFS guitar DI on the two-head rig at the owner's rate and
/// buffer, unity blocks, insert off. Every frame of the DI must come out
/// once, in order, at its own level (-6.02 dBFS peak), and the same on L and
/// R (a mono head is broadcast, invariant #5). A second head's copy, a
/// replayed buffer or a skipped buffer breaks the frame-by-frame match.
#[test]
fn a_minus_6_dbfs_di_renders_offline_once_at_minus_6_dbfs() {
    init();
    let di = di_minus_6_dbfs(2.0);
    let outcome = render_chain(
        &rig(&TWO_HEADS, false),
        RATE as f32,
        &di,
        FRAMES as usize,
        0,
    )
    .unwrap_or_else(|e| panic!("the two-head rig must render offline: {e}"));
    assert!(
        outcome.faulted_blocks.is_empty(),
        "a unity block faulted, so the render is not the chain: {:?}",
        outcome.faulted_blocks
    );
    let out = &outcome.samples;
    assert_eq!(
        out.len(),
        di.len(),
        "the render must play the DI's {} frames once, with no tail, got {}",
        di.len(),
        out.len()
    );

    let skewed = out
        .iter()
        .position(|[l, r]| (l - r).abs() > 1e-6)
        .map(|n| (n, out[n]));
    assert_eq!(
        skewed, None,
        "a mono DI through unity blocks must come out the same on L and R (frame, [L, R])"
    );

    let peaks = out[SETTLE..].iter().fold([0.0f32; 2], |p, [l, r]| {
        [p[0].max(l.abs()), p[1].max(r.abs())]
    });
    for (side, peak) in ["L", "R"].into_iter().zip(peaks) {
        let level = dbfs(peak);
        assert!(
            (level - BATTERY_PEAK_DBFS).abs() <= LEVEL_TOL_DB,
            "{side}: a -6.02 dBFS DI through unity blocks rendered at {level:+.2} dBFS \
             ({:+.2} dB; +6.02 = the two heads stacked)",
            level - BATTERY_PEAK_DBFS
        );
    }

    let (lag, err) = best_alignment(&di, out);
    assert!(
        err <= SAMPLE_TOL,
        "the render is not the DI played once in order: at its best delay ({lag} frames, \
         searched 0..={MAX_LAG}) a sample is still {err:.4} off (a replayed, skipped or \
         doubled buffer is off by the signal itself)"
    );
}

/// The latency probe must run at the conditions the chain runs at, the same
/// on every call: the HD 8's 44.1 kHz and 64-frame buffer (docs/mcp.md:
/// "probed at that chain input's real rate and buffer"). A probe that
/// switches buffer or rate between calls would report a different time for
/// the same chain, which is one way to get 8.48 then 15.62 ms.
#[test]
fn every_latency_probe_runs_at_the_rigs_rate_and_buffer() {
    let probe = Probe::new(rig(&TWO_HEADS, false));
    let conditions: Vec<(f64, f64)> = (0..16)
        .map(|_| {
            let v = probe.latency();
            (number(&v, "sample_rate"), number(&v, "buffer_frames"))
        })
        .collect();
    let rig_conditions = (f64::from(RATE), f64::from(FRAMES));
    assert!(
        conditions.iter().all(|c| *c == rig_conditions),
        "16 latency probes of the unchanged two-head chain on the HD 8 \
         ({RATE} Hz, {FRAMES} frames) ran at (sample_rate, buffer_frames) {conditions:?}"
    );
}

/// Latency never grows. On an unchanged chain the reading must not climb
/// from call to call, even with scene switches (insert off and on) and
/// quality probes between the calls, which is how the owner used the rig.
/// The median of the last quarter of the calls must stay within x1.5
/// (+0.1 ms of timer noise) of the median of the first quarter. The rig
/// measured x1.84 (8.48 -> 15.62 ms).
#[test]
fn repeated_latency_probes_of_an_unchanged_chain_do_not_grow() {
    let off = Probe::new(rig(&TWO_HEADS, false));
    let on = Probe::new(rig(&TWO_HEADS, true));
    let mut readings: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
    for _round in 0..ROUNDS {
        for (state, probe) in [&off, &on].into_iter().enumerate() {
            for _ in 0..PROBES_PER_ROUND {
                readings[state].push(number(&probe.latency(), "dsp_latency_ms"));
            }
            probe.quality();
        }
    }
    for (label, r) in ["insert off", "insert on"].into_iter().zip(&readings) {
        let quarter = r.len() / 4;
        let first = median(&r[..quarter]);
        let last = median(&r[r.len() - quarter..]);
        assert!(
            last <= first * GROWTH + SLACK_MS,
            "{label}: the dsp_latency_ms of the unchanged two-head chain grew from a \
             median of {first:.3} ms over the first {quarter} probes to {last:.3} ms over \
             the last {quarter} (x{:.2}; allowed x{GROWTH} + {SLACK_MS} ms). All readings: \
             {r:.3?}",
            last / first.max(f64::MIN_POSITIVE)
        );
    }
}
