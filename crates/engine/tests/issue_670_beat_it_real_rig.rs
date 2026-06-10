#![cfg(not(debug_assertions))]
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
//! GATING: `#![cfg(not(debug_assertions))]` — compiled out in debug, not ignored;
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

/// Issue #670: reproduce the LIVE input-callback xruns. Offline the chain is
/// light, but the live app runs the WHOLE thread soup at once — one input
/// callback AND one output callback per active chain, plus the GUI render /
/// spectrum FFT thread, all competing for the cores. This spawns that exact
/// structure with the REAL chains and measures the input callback's overrun
/// rate, the way the live meter reports `rig:input-3` overloading.
#[test]
#[cfg(target_os = "macos")]
fn live_thread_soup_reproduces_input_xruns() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    init_registry();
    let n_chains = 4usize;
    let period_ns = (BUFFER as u128 * 1_000_000_000) / SR as u128;

    // (0) The chain itself is LIGHT — every block of the real preset + the
    // full chain median is a fraction of the 64-frame budget. The crackle is
    // NOT the rig being "too heavy"; it is the audio thread being preempted.
    assert_no_faulted_blocks(&beat_it_chain());
    for b in beat_it_blocks() {
        let m = block_model(&b).to_string();
        if !m.is_empty() {
            let _ = measure(&m, &build(&isolated(&m, b)));
        }
    }
    let (light_p50, _, period_m, _) = measure("FULL chain (no contention)", &build(&beat_it_chain()));
    assert!(
        light_p50 <= period_m,
        "the Beat It chain's median per-buffer cost {}us must fit the 64-frame \
         budget {}us — it is LIGHT.",
        light_p50 / 1_000,
        period_m / 1_000,
    );

    // One run of the live thread soup: N real chains, each with an input AND
    // an output callback thread, while the GUI (Slint render + spectrum FFT +
    // meters) saturates every core. Inputs are paced ~one buffer period like
    // the real device cadence. Returns (input_overruns, input_buffers).
    let run_soup = |realtime: bool, with_ir: bool| -> (usize, usize) {
        let chains: Vec<Arc<ChainRuntimeState>> = (0..n_chains)
            .map(|_| build(&beat_it_chain_opt(with_ir)))
            .collect();
        let stop = Arc::new(AtomicBool::new(false));
        let n_gui = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8);
        let gui: Vec<_> = (0..n_gui)
            .map(|_| {
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    let mut buf = vec![0.0f64; 4096];
                    while !stop.load(Ordering::Relaxed) {
                        for (i, v) in buf.iter_mut().enumerate() {
                            *v = ((i as f64) * 0.001).sin().cos().tan().sqrt();
                        }
                        std::hint::black_box(&buf);
                    }
                })
            })
            .collect();
        let total = Arc::new(AtomicUsize::new(0));
        let over = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|s| {
            for rt in &chains {
                let rt_o = Arc::clone(rt);
                let stop_o = Arc::clone(&stop);
                s.spawn(move || {
                    let mut out = vec![0.0f32; BUFFER * 2];
                    while !stop_o.load(Ordering::Relaxed) {
                        process_output_f32(&rt_o, 0, &mut out, 2);
                        std::thread::yield_now();
                    }
                });
                let rt_i = Arc::clone(rt);
                let stop_i = Arc::clone(&stop);
                let total = Arc::clone(&total);
                let over = Arc::clone(&over);
                s.spawn(move || {
                    if realtime {
                        rt::promote(period_ns as u64);
                    }
                    let input = vec![0.1f32; BUFFER];
                    for _ in 0..256 {
                        process_input_f32(&rt_i, 0, &input, 1);
                    }
                    while !stop_i.load(Ordering::Relaxed) {
                        let t0 = Instant::now();
                        process_input_f32(&rt_i, 0, &input, 1);
                        if t0.elapsed().as_nanos() > period_ns {
                            over.fetch_add(1, Ordering::Relaxed);
                        }
                        total.fetch_add(1, Ordering::Relaxed);
                        std::thread::sleep(std::time::Duration::from_micros(900));
                    }
                });
            }
            std::thread::sleep(std::time::Duration::from_secs(6));
            stop.store(true, Ordering::Relaxed);
        });
        for h in gui {
            let _ = h.join();
        }
        (
            over.load(Ordering::Relaxed),
            total.load(Ordering::Relaxed).max(1),
        )
    };

    // The user's decisive observation: disabling the IR/CAB stops the
    // crackle. Reproduce that exact toggle under the live load — the SAME
    // chains, with vs without the IR block — and compare the input xrun rate.
    let (with_ir_over, with_ir_total) = run_soup(false, true);
    let (no_ir_over, no_ir_total) = run_soup(false, false);
    eprintln!(
        "[#670 beat-it] LIVE-SOUP {n_chains} chains under GUI load: \
         WITH IR xruns={with_ir_over}/{with_ir_total} ({:.1}%)  \
         WITHOUT IR xruns={no_ir_over}/{no_ir_total} ({:.1}%)",
        with_ir_over as f64 / with_ir_total as f64 * 100.0,
        no_ir_over as f64 / no_ir_total as f64 * 100.0,
    );

    // NOTE: offline this does NOT reproduce the user's live observation —
    // WITH and WITHOUT the IR come out within run-to-run noise. The IR is
    // ~4us, allocates nothing, and runs flush-to-zero-protected here, so its
    // live effect is something this offline harness does not capture (it has
    // no real cpal/CoreAudio path, no real decaying signal, no real Slint
    // render). Left as a diagnostic, not an assertion, until the live IR cost
    // is measured directly. The one robust fact this test pins: under heavy
    // load the unprotected input thread DOES overrun (a few %), the live
    // crackle mechanism.
    assert!(
        with_ir_over + no_ir_over > 0,
        "expected SOME input overruns under the saturating load (the crackle \
         mechanism); got WITH ir={with_ir_over}, WITHOUT ir={no_ir_over}"
    );
}

/// Issue #670: render the FULL Beat It chain offline with a decaying note (as
/// when actually playing) WITH vs WITHOUT the IR, and inspect the OUTPUT for
/// the "caixa de abelha" — sustained buzz/energy in the tail where it should
/// be decaying to silence. The user proved disabling the IR stops it, so the
/// difference must be in the rendered audio, not the timing.
#[test]
fn beat_it_output_tail_is_clean_with_ir() {
    init_registry();

    // A plucked note: 110 Hz decaying over ~0.4 s, then 0.6 s of silence so
    // the tail (reverb + IR) is fully exposed.
    let attack = (SR * 0.4) as usize;
    let total = (SR * 1.0) as usize;
    let input: Vec<[f32; 2]> = (0..total)
        .map(|i| {
            let t = i as f32 / SR;
            let env = if i < attack { (-t * 6.0).exp() } else { 0.0 };
            let s = 0.4 * env * (2.0 * std::f32::consts::PI * 110.0 * t).sin();
            [s, s]
        })
        .collect();

    let analyze = |with_ir: bool| -> (f32, f32, f32, usize) {
        let chain = beat_it_chain_opt(with_ir);
        let out = render_chain(&chain, SR, &input, BUFFER, 0)
            .expect("render")
            .samples;
        // Tail window: the last 0.4 s, where the note is long gone.
        let tail_start = out.len().saturating_sub((SR * 0.4) as usize);
        let tail = &out[tail_start..];
        let mut peak = 0.0f32;
        let mut energy = 0.0f64;
        let mut disc = 0.0f64; // sum |s[n]-s[n-1]| — high-frequency "buzz" content
        let mut nan = 0usize;
        let mut prev = 0.0f32;
        for s in tail {
            let v = s[0];
            if !v.is_finite() {
                nan += 1;
                continue;
            }
            peak = peak.max(v.abs());
            energy += (v * v) as f64;
            disc += (v - prev).abs() as f64;
            prev = v;
        }
        let rms = (energy / tail.len() as f64).sqrt() as f32;
        // buzz ratio: high-frequency content relative to amplitude. A smooth
        // decaying tail has low disc/peak; a buzz has high.
        let buzz = (disc / tail.len() as f64) as f32 / peak.max(1e-9);
        (peak, rms, buzz, nan)
    };

    let (peak_ir, rms_ir, buzz_ir, nan_ir) = analyze(true);
    let (peak_no, rms_no, buzz_no, nan_no) = analyze(false);
    eprintln!(
        "[#670 IR] TAIL with-IR: peak={peak_ir:.4} rms={rms_ir:.5} buzz={buzz_ir:.3} nan={nan_ir}  |  \
         no-IR: peak={peak_no:.4} rms={rms_no:.5} buzz={buzz_no:.3} nan={nan_no}"
    );

    assert_eq!(nan_ir, 0, "BUG #670: IR tail has non-finite samples (NaN/Inf)");
    assert!(
        buzz_ir < buzz_no.max(0.05) * 2.0,
        "BUG #670: with the IR the output tail is much buzzier (buzz={buzz_ir:.3}) \
         than without it (buzz={buzz_no:.3}) — high-frequency junk in a tail that \
         should be decaying smoothly. That is the 'caixa de abelha'."
    );
}

/// Issue #670: drive the chain through the LIVE elastic-buffered path
/// (process_input_f32 fills the elastic ring, process_output_f32 pops it) —
/// which `render_chain` bypasses — and inspect the popped OUTPUT for the
/// beehive. The IR adds latency; if that desyncs the elastic so the output
/// pops silence/stale frames mid-tone, the result is a buzzy click-train.
#[test]
fn beat_it_elastic_output_is_clean_with_ir() {
    init_registry();

    let analyze = |with_ir: bool| -> (usize, f32, usize) {
        let rt = build(&beat_it_chain_opt(with_ir));
        // steady 110 Hz tone fed buffer-by-buffer through the live path.
        let mut out_all: Vec<f32> = Vec::new();
        let mut out = vec![0.0_f32; BUFFER * 2];
        let mut phase = 0usize;
        for _ in 0..2048 {
            let input: Vec<f32> = (0..BUFFER)
                .map(|_| {
                    let s = 0.4 * (2.0 * std::f32::consts::PI * 110.0 * phase as f32 / SR).sin();
                    phase += 1;
                    s
                })
                .collect();
            process_input_f32(&rt, 0, &input, 1);
            process_output_f32(&rt, 0, &mut out, 2);
            out_all.extend(out.iter().step_by(2)); // left channel
        }
        // Skip warmup, analyze steady state.
        let tail = &out_all[out_all.len() / 2..];
        let peak = tail.iter().fold(0.0f32, |a, &v| a.max(v.abs()));
        // Silent frames in the middle of a sustained tone = elastic underrun
        // dropouts (the click-train / buzz).
        let silent = tail.iter().filter(|&&v| v.abs() < peak * 0.001).count();
        // Discontinuity (high-frequency junk).
        let mut disc = 0.0f64;
        let mut prev = 0.0f32;
        for &v in tail {
            disc += (v - prev).abs() as f64;
            prev = v;
        }
        let buzz = (disc / tail.len() as f64) as f32 / peak.max(1e-9);
        (silent, buzz, tail.len())
    };

    let (sil_ir, buzz_ir, n) = analyze(true);
    let (sil_no, buzz_no, _) = analyze(false);
    eprintln!(
        "[#670 IR] ELASTIC out (n={n}): with-IR silent_frames={sil_ir} buzz={buzz_ir:.3}  |  \
         no-IR silent_frames={sil_no} buzz={buzz_no:.3}"
    );

    assert!(
        sil_ir <= sil_no + n / 100,
        "BUG #670: with the IR the live output has {sil_ir} silent dropout frames \
         vs {sil_no} without it (>1% more) — the IR desyncs the elastic buffer and \
         the output pops silence mid-tone (the buzz)."
    );
}

/// Issue #670: the user pinned the beehive to NAM A2 + IR/CAB TOGETHER. Test
/// that combination under cold-cache pressure (the live UI eviction): build
/// input -> NAM -> IR -> output and measure per-buffer cost HOT vs with the
/// cache evicted before each callback, and compare NAM-only / IR-only / both.
/// If "both" is super-linear under cold cache, they evict each other's working
/// set and reload from memory every buffer.
#[test]
fn nam_plus_ir_cold_cache_cost() {
    init_registry();
    let blocks = beat_it_blocks();
    let nam = blocks
        .iter()
        .find(|b| block_model(b).starts_with("nam_fender_deluxe"))
        .or_else(|| blocks.iter().find(|b| matches!(&b.kind, AudioBlockKind::Nam(_))))
        .expect("preset has a NAM")
        .clone();
    let ir = blocks
        .iter()
        .find(|b| block_model(b).starts_with("ir_"))
        .expect("preset has an IR")
        .clone();

    let chain = |bs: Vec<AudioBlock>| -> Chain {
        let mut blocks = vec![input_mono()];
        blocks.extend(bs);
        blocks.push(output_stereo());
        Chain {
            id: ChainId("nam-ir".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            blocks,
        }
    };

    let measure = |label: &str, c: &Chain| {
        let rt = build(c);
        let input = vec![0.1_f32; BUFFER];
        let mut out = vec![0.0_f32; BUFFER * 2];
        for _ in 0..256 {
            process_input_f32(&rt, 0, &input, 1);
            process_output_f32(&rt, 0, &mut out, 2);
        }
        let mut p50 = |pollute: &mut [u8]| -> u128 {
            let mut s = Vec::with_capacity(2000);
            for _ in 0..2000 {
                if !pollute.is_empty() {
                    let mut x = 1u8;
                    let mut i = 0;
                    while i < pollute.len() {
                        pollute[i] = pollute[i].wrapping_add(x);
                        x = x.wrapping_add(13);
                        i += 64;
                    }
                    std::hint::black_box(&pollute);
                }
                let t0 = Instant::now();
                process_input_f32(&rt, 0, &input, 1);
                process_output_f32(&rt, 0, &mut out, 2);
                s.push(t0.elapsed().as_nanos());
            }
            s.sort_unstable();
            s[s.len() / 20]
        };
        let hot = p50(&mut []);
        let mut big = vec![0u8; 64 * 1024 * 1024];
        let cold = p50(&mut big);
        eprintln!(
            "[#670 NAM+IR] {label:<10} hot={}us cold={}us ratio={:.1}x",
            hot / 1000,
            cold / 1000,
            cold as f64 / hot.max(1) as f64
        );
    };

    measure("NAM only", &chain(vec![nam.clone()]));
    measure("IR only", &chain(vec![ir.clone()]));
    measure("NAM+IR", &chain(vec![nam.clone(), ir.clone()]));
}

/// Issue #670 — the user's test, their way: wire the REAL Beat It chain
/// (2 NAM A2 + IR + reverb…) through the internal runtime, play a note that
/// DECAYS to silence (a real pluck ringing out — the chain's reverb/IR/NAM
/// tails decay through the subnormal float range), process buffer-by-buffer at
/// 64 frames, and assert NO buffer blows the 64-frame deadline. A blown buffer
/// IS the "audio overload" warning the user sees. A steady tone never reaches
/// the subnormal regime; a decaying tail does.
#[test]
fn beat_it_decaying_note_never_overruns_the_deadline() {
    init_registry();
    let chain = beat_it_chain();
    // Prove the COMPLETE preset is live — a faulted block degrades to a cheap
    // pass-through and would understate the cost.
    assert_no_faulted_blocks(&chain);
    let live: Vec<&str> = chain.blocks.iter().map(block_model).filter(|m| !m.is_empty()).collect();
    eprintln!("[#670 DECAY] live blocks ({}): {:?}", live.len(), live);
    let rt = build(&chain);
    let deadline_ns: u128 = (BUFFER as u128 * 1_000_000_000) / SR as u128; // 1333us @64/48k

    let mut out = vec![0.0_f32; BUFFER * 2];
    // Warm up.
    for _ in 0..256 {
        let warm = vec![0.1_f32; BUFFER];
        process_input_f32(&rt, 0, &warm, 1);
        process_output_f32(&rt, 0, &mut out, 2);
    }

    // ~3 s: a 110 Hz note plucked at 0.6 and decaying exponentially down
    // THROUGH the subnormal range, exactly like letting a chord ring out.
    let total_buffers = (SR as usize * 3) / BUFFER;
    let mut phase = 0usize;
    let mut worst_ns = 0u128;
    let mut overruns = 0usize;
    let mut worst_at_s = 0.0f64;

    for b in 0..total_buffers {
        let t0_s = (b * BUFFER) as f32 / SR;
        let input: Vec<f32> = (0..BUFFER)
            .map(|_| {
                let t = phase as f32 / SR;
                let env = (-t * 6.0).exp(); // 0.6 -> subnormal over ~3 s
                let s = 0.6 * env * (2.0 * std::f32::consts::PI * 110.0 * phase as f32 / SR).sin();
                phase += 1;
                s
            })
            .collect();

        let t0 = Instant::now();
        process_input_f32(&rt, 0, &input, 1);
        process_output_f32(&rt, 0, &mut out, 2);
        let ns = t0.elapsed().as_nanos();

        if ns > worst_ns {
            worst_ns = ns;
            worst_at_s = t0_s as f64;
        }
        if ns > deadline_ns {
            overruns += 1;
        }
    }

    eprintln!(
        "[#670 DECAY] 2 NAM A2 + IR, decaying note: worst buffer={}us @{:.2}s  deadline={}us  overruns={}/{}  recorded_xruns={}",
        worst_ns / 1000,
        worst_at_s,
        deadline_ns / 1000,
        overruns,
        total_buffers,
        rt.xrun_count(),
    );

    assert_eq!(
        overruns, 0,
        "BUG #670: the Beat It chain (2 NAM A2 + IR) blew the 64-frame deadline \
         on {overruns} buffers while a note decayed to silence (worst {}us vs \
         {}us budget @{:.2}s into the decay) — the audio overload the user hears. \
         A decaying tail generates subnormals; if any block runs them without \
         flush-to-zero the FP stall blows the buffer.",
        worst_ns / 1000,
        deadline_ns / 1000,
        worst_at_s,
    );
}

fn load_di_loop_mono(name: &str) -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/di-loops")
        .join(name);
    let mut reader = hound::WavReader::open(&path)
        .unwrap_or_else(|e| panic!("open DI loop {}: {e}", path.display()));
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap() as f32 / max).collect()
        }
    };
    if spec.channels == 2 {
        interleaved.iter().step_by(2).copied().collect()
    } else {
        interleaved
    }
}

/// Issue #670 — the user's way: drive the COMPLETE Beat It chain through the
/// internal runtime feeding a REAL guitar DI recording (not a synthetic tone),
/// at 64 frames, and fail if ANY buffer blows the deadline (the audio
/// overload). Real playing has the transients, dynamics and decays a clean
/// sine never produces.
#[test]
fn beat_it_real_guitar_di_never_overruns() {
    init_registry();
    let chain = beat_it_chain();
    assert_no_faulted_blocks(&chain);
    let rt = build(&chain);
    let deadline_ns: u128 = (BUFFER as u128 * 1_000_000_000) / SR as u128;

    let di = load_di_loop_mono("clean-electric-guitar-loop.wav");
    let mut out = vec![0.0_f32; BUFFER * 2];
    // Warm up.
    for _ in 0..256 {
        process_input_f32(&rt, 0, &di[..BUFFER], 1);
        process_output_f32(&rt, 0, &mut out, 2);
    }

    let mut worst_ns = 0u128;
    let mut overruns = 0usize;
    let mut buffers = 0usize;
    // Play the loop 4 times (~22 s) so reverb/IR tails build and decay.
    for _ in 0..4 {
        for chunk in di.chunks(BUFFER) {
            let mut input = vec![0.0_f32; BUFFER];
            input[..chunk.len()].copy_from_slice(chunk);
            let t0 = Instant::now();
            process_input_f32(&rt, 0, &input, 1);
            process_output_f32(&rt, 0, &mut out, 2);
            let ns = t0.elapsed().as_nanos();
            worst_ns = worst_ns.max(ns);
            if ns > deadline_ns {
                overruns += 1;
            }
            buffers += 1;
        }
    }

    eprintln!(
        "[#670 GUITAR] real DI through full Beat It: worst buffer={}us  deadline={}us  overruns={}/{}  recorded_xruns={}",
        worst_ns / 1000,
        deadline_ns / 1000,
        overruns,
        buffers,
        rt.xrun_count(),
    );
    assert_eq!(
        overruns, 0,
        "BUG #670: the full Beat It chain blew the deadline on {overruns} \
         buffers playing a REAL guitar DI (worst {}us vs {}us) — the audio \
         overload the user hears.",
        worst_ns / 1000,
        deadline_ns / 1000,
    );
}

/// Issue #670 — play the Green Day DI through the full Beat It chain and
/// ANALYZE which buffers are heavy: correlate each buffer's processing time
/// with its input level, so a heavy buffer on a QUIET passage points to a
/// denormal stall while a heavy buffer on a LOUD passage points to the NAM's
/// nonlinear cost. Fails if any buffer blows the 64-frame deadline.
#[test]
fn beat_it_green_day_di_analysis() {
    init_registry();
    let chain = beat_it_chain();
    assert_no_faulted_blocks(&chain);
    let rt = build(&chain);
    let deadline_ns: u128 = (BUFFER as u128 * 1_000_000_000) / SR as u128;

    let di = load_di_loop_mono("phil-STRATO-green_day.wav");
    eprintln!(
        "[#670 GREENDAY] DI: {} samples ({:.1}s @48k), peak={:.3}",
        di.len(),
        di.len() as f32 / SR,
        di.iter().fold(0.0f32, |a, &v| a.max(v.abs())),
    );

    let mut out = vec![0.0_f32; BUFFER * 2];
    for _ in 0..256 {
        process_input_f32(&rt, 0, &di[..BUFFER.min(di.len())], 1);
        process_output_f32(&rt, 0, &mut out, 2);
    }

    // (time_ns, input_rms, second) per buffer.
    let mut rows: Vec<(u128, f32, f32)> = Vec::new();
    let mut overruns = 0usize;
    for pass in 0..1 {
        for (ci, chunk) in di.chunks(BUFFER).enumerate() {
            let mut input = vec![0.0_f32; BUFFER];
            input[..chunk.len()].copy_from_slice(chunk);
            let rms = (input.iter().map(|&v| v * v).sum::<f32>() / BUFFER as f32).sqrt();
            let t0 = Instant::now();
            process_input_f32(&rt, 0, &input, 1);
            process_output_f32(&rt, 0, &mut out, 2);
            let ns = t0.elapsed().as_nanos();
            if ns > deadline_ns {
                overruns += 1;
            }
            let sec = (pass * di.len() + ci * BUFFER) as f32 / SR;
            rows.push((ns, rms, sec));
        }
    }

    let total = rows.len();
    let mut by_time = rows.clone();
    by_time.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    let p50 = {
        let mut t: Vec<u128> = rows.iter().map(|r| r.0).collect();
        t.sort_unstable();
        t[t.len() / 2]
    };
    eprintln!(
        "[#670 GREENDAY] {total} buffers  p50={}us  worst={}us  deadline={}us  overruns={overruns}  xruns={}",
        p50 / 1000,
        by_time[0].0 / 1000,
        deadline_ns / 1000,
        rt.xrun_count(),
    );
    eprintln!("[#670 GREENDAY] 12 heaviest buffers (time_us, input_rms, at_s):");
    for r in by_time.iter().take(12) {
        eprintln!("    {:>5}us  rms={:.5}  @{:.2}s", r.0 / 1000, r.1, r.2);
    }

    assert_eq!(
        overruns, 0,
        "BUG #670: Green Day DI blew the deadline on {overruns}/{total} buffers \
         (worst {}us vs {}us).",
        by_time[0].0 / 1000,
        deadline_ns / 1000,
    );
}
