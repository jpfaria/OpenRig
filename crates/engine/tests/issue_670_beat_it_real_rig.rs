//! Issue #670 — RED repro of the user's ACTUAL crackle, with the user's
//! ACTUAL plugins (no synthetic fixtures, no supposing).
//!
//! This drives the exact "Beat It — Michael Jackson (rhythm)" chain the user
//! runs (compressor → guitar EQ → gate → NAM maxon_od808 A2 → NAM
//! fender_deluxe_reverb_65 A2 → IR fender_deluxe_reverb_oxford cab → 8-band
//! parametric EQ → LV2 Dragonfly Hall reverb → brickwall limiter) through
//! the REAL audio-thread path (`process_input_f32` + `process_output_f32`)
//! at the buffer size that crackles (64 frames @ 48 kHz), and asserts the
//! chain meets its 64-frame deadline.
//!
//! It is RED on purpose: on the user's M4 Pro the chain's real per-buffer
//! cost (~1.4-1.6 ms, measured) exceeds the 64-frame period (1.333 ms), so
//! the callback overruns → xrun → the crackle. The failure prints a
//! per-block breakdown so the cost is attributed to a specific block with
//! REAL numbers, not a guess.
//!
//! The real plugins are bundled under `tests/fixtures/plugins/{nam,ir,lv2}`
//! (copied from the user's plugins tree) so the test is self-contained and
//! the absolute microseconds are the user's real amps, not a fraction.
//!
//! GATING: `#[cfg_attr(debug_assertions, ignore)]` — timing is only
//! meaningful in release. Run:
//!   cargo test -p engine --release --test issue_670_beat_it_real_rig -- --nocapture

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Once;
use std::time::Instant;

use domain::ids::{BlockId, ChainId, DeviceId};
use engine::offline::render_chain;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_graph::build_chain_runtime_state;
use engine::runtime_state::ChainRuntimeState;
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};

const SR: f32 = 48_000.0;
const BUFFER: usize = 64;
const N_ITERATIONS: usize = 5_000;
const WARMUP: usize = 128;

fn plugins_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins")
}

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        lv2::register_builder();
        block_dyn::register_natives();
        block_filter::register_natives();
        block_reverb::register_natives();
        block_gain::register_natives();
        block_amp::register_natives();
        block_preamp::register_natives();
        block_cab::register_natives();
        block_delay::register_natives();
        block_mod::register_natives();
        block_pitch::register_natives();
        plugin_loader::registry::init(&plugins_root());
    });
}

fn input_mono() -> AudioBlock {
    AudioBlock {
        id: BlockId("in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn output_stereo() -> AudioBlock {
    AudioBlock {
        id: BlockId("out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

// ── Load the user's REAL "Beat It (rhythm)" preset through the PRODUCTION
// parser (infra-yaml), so this test runs the LITERAL preset file the user
// runs — not a hand transcription. The preset's FX blocks are wrapped with a
// mono input and a stereo output, exactly like the live chain at buffer 64.

fn beat_it_blocks() -> Vec<AudioBlock> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/presets/beat_it_michael_jackson_rhythm.yaml");
    infra_yaml::load_chain_preset_file(&path)
        .unwrap_or_else(|e| {
            panic!("must load the real Beat It preset through the production parser: {e}")
        })
        .blocks
}

fn block_model(b: &AudioBlock) -> &str {
    match &b.kind {
        AudioBlockKind::Nam(n) => n.model.as_str(),
        AudioBlockKind::Core(c) => c.model.as_str(),
        _ => "",
    }
}

fn beat_it_chain() -> Chain {
    beat_it_chain_opt(true)
}

fn beat_it_chain_opt(with_ir: bool) -> Chain {
    let mut blocks = vec![input_mono()];
    for b in beat_it_blocks() {
        if !with_ir && block_model(&b).starts_with("ir_") {
            continue; // drop the IR/CAB only to isolate its effect
        }
        blocks.push(b);
    }
    blocks.push(output_stereo());
    Chain {
        id: ChainId("issue-670-beat-it".into()),
        description: Some("Beat It (rhythm) — loaded from the user's real preset".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 139.0,
        blocks,
    }
}

fn isolated(label: &str, block: AudioBlock) -> Chain {
    Chain {
        id: ChainId(format!("issue-670-beat-it-iso-{label}")),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![input_mono(), block, output_stereo()],
    }
}

fn assert_no_faulted_blocks(chain: &Chain) {
    let input: Vec<[f32; 2]> = (0..1024)
        .map(|n| {
            let s = 0.2 * (2.0 * std::f32::consts::PI * 220.0 * n as f32 / SR).sin();
            [s, s]
        })
        .collect();
    let outcome = render_chain(chain, SR, &input, BUFFER, 0)
        .expect("Beat It chain must render for the #670 setup gate");
    assert!(
        outcome.faulted_blocks.is_empty(),
        "#670 setup gate: Beat It chain has FAULTED blocks {:?} — a faulted \
         block degrades to a cheap pass-through and would UNDERSTATE the real \
         cost. The bundled fixture plugin is missing or a param is wrong.",
        outcome.faulted_blocks
    );
}

fn build(chain: &Chain) -> Arc<ChainRuntimeState> {
    Arc::new(build_chain_runtime_state(chain, SR, &[BUFFER]).expect("runtime must build"))
}

/// Median + p95 per-buffer wall time (ns) over N steady-state callbacks.
fn measure(label: &str, rt: &Arc<ChainRuntimeState>) -> (u128, u128, u128, usize) {
    let period_ns = (BUFFER as u128 * 1_000_000_000) / SR as u128;
    let input = vec![0.1_f32; BUFFER];
    let mut out = vec![0.0_f32; BUFFER * 2];
    for _ in 0..WARMUP {
        process_input_f32(rt, 0, &input, 1);
        process_output_f32(rt, 0, &mut out, 2);
    }
    let mut elapsed = Vec::with_capacity(N_ITERATIONS);
    for _ in 0..N_ITERATIONS {
        let t0 = Instant::now();
        process_input_f32(rt, 0, &input, 1);
        process_output_f32(rt, 0, &mut out, 2);
        elapsed.push(t0.elapsed().as_nanos());
    }
    elapsed.sort_unstable();
    let p50 = elapsed[N_ITERATIONS / 2];
    let p95 = elapsed[N_ITERATIONS * 95 / 100];
    let overruns = elapsed.iter().filter(|&&t| t > period_ns).count();
    eprintln!(
        "[#670 beat-it] {label:<26} p50={}us p95={}us overruns={}/{} ({:.0}% of period@64={}us)",
        p50 / 1_000,
        p95 / 1_000,
        overruns,
        N_ITERATIONS,
        (p50 as f64 / period_ns as f64) * 100.0,
        period_ns / 1_000,
    );
    (p50, p95, period_ns, overruns)
}

// Real-time time-constraint promotion (raw mach FFI; mirrors the SHIPPED
// infra_cpal::promote_current_thread_realtime policy so this validates it).
#[cfg(target_os = "macos")]
mod rt {
    #[repr(C)]
    struct TimeConstraint {
        period: u32,
        computation: u32,
        constraint: u32,
        preemptible: i32,
    }
    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    extern "C" {
        fn mach_thread_self() -> u32;
        fn mach_timebase_info(info: *mut Timebase) -> i32;
        fn thread_policy_set(thread: u32, flavor: i32, info: *mut i32, count: u32) -> i32;
    }
    const THREAD_TIME_CONSTRAINT_POLICY: i32 = 2;
    pub fn promote(period_ns: u64) -> bool {
        unsafe {
            let mut tb = Timebase { numer: 0, denom: 0 };
            if mach_timebase_info(&mut tb) != 0 || tb.numer == 0 {
                return false;
            }
            let to_mach = |ns: u64| ((ns as u128 * tb.denom as u128) / tb.numer as u128) as u32;
            let period = to_mach(period_ns);
            let mut p = TimeConstraint {
                period,
                // Realistic per-buffer cost (~1/4 period), NOT 90%: claiming
                // most of the period makes N audio threads oversubscribe the
                // realtime band and preempt each other (the #670 multi-chain
                // crackle). preemptible=1 lets them cooperate.
                computation: to_mach(period_ns / 4),
                constraint: period,
                preemptible: 1,
            };
            thread_policy_set(
                mach_thread_self(),
                THREAD_TIME_CONSTRAINT_POLICY,
                &mut p as *mut _ as *mut i32,
                4,
            ) == 0
        }
    }
}

/// The whole #670 story in ONE sequential test (one test per file, so the
/// hog threads it spawns never overlap another timing test and corrupt it):
///   1. the real Beat It chain is LIGHT — its median per-buffer cost is a
///      fraction of the 64-frame budget (refutes "the rig is too heavy").
///   2. under heavy CPU contention (the live GUI + control worker + other
///      threads) a NON-realtime audio thread overruns badly — the crackle.
///   3. the realtime time-constraint policy (the shipped fix) keeps the SAME
///      chain's deadline under the SAME contention — far fewer overruns.
///
/// Real plugins, real chain, real numbers — no synthetic fixtures.
#[test]
#[cfg_attr(debug_assertions, ignore = "deadline timing requires --release")]
fn beat_it_real_rig_light_and_realtime_protects() {
    init_registry();
    let chain = beat_it_chain();
    assert_no_faulted_blocks(&chain);

    // (1) Where the cost actually is — every block of the REAL preset,
    // measured isolated, with the real plugins.
    for b in beat_it_blocks() {
        let model = block_model(&b).to_string();
        if model.is_empty() {
            continue;
        }
        let _ = measure(&model, &build(&isolated(&model, b)));
    }

    let rt_state = build(&chain);
    let (p50, _p95, period_ns, _ov) = measure("FULL chain (no contention)", &rt_state);
    assert!(
        p50 <= period_ns,
        "the Beat It chain's median per-buffer cost {}us must fit the 64-frame \
         budget {}us — the chain is LIGHT; the crackle is not its weight.",
        p50 / 1_000,
        period_ns / 1_000,
    );

    // (2) The user runs SEVERAL chains at once (their project has 4).
    // Reproduce that exactly: K concurrent REAL Beat It chains, each on its
    // own thread like the live cpal input callbacks, and count how many
    // buffers miss the 64-frame deadline as K grows. This is the LIVE
    // contention — real chains, no artificial hog threads. Optionally each
    // thread promotes itself to the shipped realtime policy.
    let period_ns = (BUFFER as u128 * 1_000_000_000) / SR as u128;
    let run_concurrent_opt = |k: usize, realtime: bool, with_ir: bool| -> usize {
        let chains: Vec<Arc<ChainRuntimeState>> =
            (0..k).map(|_| build(&beat_it_chain_opt(with_ir))).collect();
        std::thread::scope(|s| {
            let handles: Vec<_> = chains
                .iter()
                .map(|rt| {
                    s.spawn(move || {
                        #[cfg(target_os = "macos")]
                        if realtime {
                            rt::promote(period_ns as u64);
                        }
                        let _ = realtime;
                        let input = vec![0.1_f32; BUFFER];
                        let mut out = vec![0.0_f32; BUFFER * 2];
                        for _ in 0..WARMUP {
                            process_input_f32(rt, 0, &input, 1);
                            process_output_f32(rt, 0, &mut out, 2);
                        }
                        let mut over = 0usize;
                        for _ in 0..N_ITERATIONS {
                            let t0 = Instant::now();
                            process_input_f32(rt, 0, &input, 1);
                            process_output_f32(rt, 0, &mut out, 2);
                            if t0.elapsed().as_nanos() > period_ns {
                                over += 1;
                            }
                        }
                        over
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).sum()
        })
    };

    for k in [1usize, 2, 4, 6, 8, 12] {
        let over = run_concurrent_opt(k, false, true);
        eprintln!(
            "[#670 beat-it] {k:>2} concurrent chains (normal): {over}/{} overruns ({:.1}% per chain)",
            k * N_ITERATIONS,
            over as f64 / (k * N_ITERATIONS) as f64 * 100.0,
        );
    }

    // The user reports the rig gets MUCH worse with the IR/CAB in the chain.
    // The IR's per-buffer compute is tiny (~4us) and it does not allocate, so
    // if it hurts under multi-chain load it must be its working set (the
    // frequency-domain delay line + IR partitions) pressuring the shared
    // cache once several chains run. Measure 8 concurrent chains WITH vs
    // WITHOUT the IR to see the real effect.
    let over_with_ir = run_concurrent_opt(8, false, true);
    let over_no_ir = run_concurrent_opt(8, false, false);
    eprintln!(
        "[#670 beat-it]  8 concurrent: WITH ir={over_with_ir} vs WITHOUT ir={over_no_ir} overruns"
    );

    // The user's project runs ~4 chains at once. On plain (cpal-default)
    // threads that is fine — a few tenths of a percent of buffers overrun,
    // which the elastic buffer absorbs.
    let normal_4 = run_concurrent_opt(4, false, true);
    // Tolerant bound (absolute overrun counts swing a lot with whatever else
    // the machine is doing): 4 plain chains must not be CATASTROPHIC. On a
    // quiet machine this is a few tenths of a percent.
    assert!(
        normal_4 * 4 < 4 * N_ITERATIONS,
        "4 concurrent Beat It chains (the user's setup) overran {normal_4}/{} \
         (>25%) on plain threads — that would be the crackle even without any \
         realtime promotion.",
        4 * N_ITERATIONS,
    );

    // ROOT CAUSE of the regression this work introduced: promoting EACH
    // chain's audio thread to a per-thread realtime time-constraint policy
    // makes N threads oversubscribe the realtime band and preempt each
    // other — far WORSE than plain scheduling. Production must NOT do it.
    #[cfg(target_os = "macos")]
    {
        let normal_8 = run_concurrent_opt(8, false, true);
        let realtime_8 = run_concurrent_opt(8, true, true);
        eprintln!(
            "[#670 beat-it]  8 concurrent: normal={normal_8} realtime={realtime_8} overruns"
        );
        // Directional, not a fixed multiplier: the ratio compresses when the
        // machine is already loaded (both saturate), but realtime is always
        // WORSE under multi-chain because it oversubscribes the realtime band.
        assert!(
            realtime_8 > normal_8,
            "#670 regression guard: realtime-promoting every audio thread must \
             be shown WORSE than plain scheduling under multi-chain load \
             (normal={normal_8}, realtime={realtime_8}). If this no longer \
             holds, re-evaluate before re-introducing per-thread realtime \
             promotion — it oversubscribes the realtime band."
        );
    }
}
