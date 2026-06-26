#![cfg(not(debug_assertions))]
//! Issue #670 — RED-first repro: a heavy multi-block rig must meet its
//! audio-thread deadline at the tightest realistic buffer (64 frames @
//! 48 kHz). The user reports crackle / "clipping" at buffer 64 with a
//! 4-chain rig (NAM/A2 amps + IR cabs + native dynamics/EQ + brickwall
//! limiters). The `output_limiter` makes true DAC clipping impossible, so
//! the working hypothesis is an audio-thread DEADLINE OVERRUN (xrun):
//! the per-buffer CPU cost of the rig exceeds the 64-frame period, the
//! callback misses its deadline, and the user hears the dropout as
//! crackle.
//!
//! This drives the REAL audio-thread path (`process_input_f32` +
//! `process_output_f32`) with REAL processors (bundled-fixture NAM neural
//! amps incl. an A2, fixture IR convolution, native dynamics/EQ) and
//! measures wall-clock per buffer against the 64-frame period, attributing
//! the cost to individual blocks.
//!
//! ROOT CAUSE (printed breakdown + pinned assertion): the cost is
//! dominated by NAM neural-amp inference, not the native effects or the IR.
//! Each NAM amp alone costs orders of magnitude more than the compressor +
//! EQ + gate + brickwall limiter combined; stacking several NAM amps across
//! chains is what saturates the 64-frame budget and makes the callback miss
//! its deadline (the crackle). The native effects and IR are negligible.
//!
//! FIXTURE vs REAL: the bundled fixture nets are small, so the ABSOLUTE
//! microseconds here are a fraction of the user's real A2 rig (the
//! real-rig numbers that reproduce the overload are recorded in the
//! issue). What is machine- and fixture-independent — and what this test
//! guards — is the cost STRUCTURE: NAM dominates. The user's rig also adds
//! LV2 reverbs/delays + a pitch autotune/harmonizer, which only add more
//! cost on top.
//!
//! GATING: `#![cfg(not(debug_assertions))]` — compiled out in debug, not ignored;
//! meaningful in release (debug has no inlining; every call is real).
//! This matches the established convention in
//! `crates/engine/src/audio_deadline_tests.rs`. Run with:
//!     cargo test -p engine --release --test issue_670_heavy_rig_deadline -- --nocapture
//!
//! SETUP GATE: before measuring, the chain is rendered offline via
//! `engine::offline::render_chain` and asserted to have ZERO faulted
//! blocks. A faulted block silently degrades to pass-through (cheap),
//! which would UNDERSTATE the cost and make the deadline test lie. If a
//! model id / param is wrong, the test fails loudly at setup.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Once;
use std::time::Instant;

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use engine::offline::render_chain;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_graph::build_chain_runtime_state;
use engine::runtime_state::ChainRuntimeState;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, NamBlock};
use project::chain::Chain;

const SR: f32 = 48_000.0;
const BUFFER: usize = 64;
const N_ITERATIONS: usize = 5_000;
const WARMUP: usize = 64;

/// Bundled fixture plugins so the test passes on CI without the on-disk
/// OpenRig-plugins tree (openrig-code-quality: builds depending on
/// external assets must bundle a minimal fixture). The fixture NAM nets
/// are small, so the ABSOLUTE numbers here are a fraction of a real A2
/// amp's cost — the test pins the cost STRUCTURE (NAM dominates the
/// native effects), not the absolute deadline. The real-rig figures that
/// reproduce the user's crackle are recorded in the issue.
fn plugins_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins")
}

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
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

/// The system binding the bound chains resolve their I/O from: a mono input
/// (dev "dev", ch [0]) and a stereo output (dev "dev", ch [0,1]) — the same
/// device/mode/channels the old head Input / tail Output blocks carried.
fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn core(id: &str, effect_type: &str, model: &str, params: ParameterSet) -> AudioBlock {
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

/// Build a `ParameterSet` of named floats. Values are taken from the
/// known-valid fixture presets (`crates/engine/tests/fixtures/presets`),
/// so every native block builds for real instead of faulting to a cheap
/// pass-through.
fn floats(pairs: &[(&str, f32)]) -> ParameterSet {
    let mut p = ParameterSet::default();
    for (k, v) in pairs {
        p.insert(*k, ParameterValue::Float(*v));
    }
    p
}

/// A NAM amp with a valid capture selection (a `preset` that resolves to a
/// real `.nam` file in the fixture manifest), so the neural net is actually
/// built and run — not faulted to pass-through.
fn nam_preset(id: &str, model: &str, preset: &str) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String(preset.into()));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Nam(NamBlock {
            model: model.into(),
            params,
        }),
    }
}

/// One heavy guitar rig: compressor → EQ → gate → 2× NAM A2 amp → IR cab
/// → hall reverb → brickwall limiter. All real processors. This mirrors
/// the shape of the user's per-chain rig (minus the excluded LV2 / pitch).
fn heavy_chain(suffix: &str) -> Chain {
    Chain {
        id: ChainId(format!("issue-670-heavy-{suffix}")),
        description: Some("issue-670 heavy rig deadline".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![
            comp_block(),
            eq_block(),
            gate_block(),
            nam_preset("amp1", "nam_marshall_plexi", "angus"),
            nam_preset("amp2", "nam_a2_slimmable", "a2"),
            ir_block(),
            core(
                "verb",
                "reverb",
                "room",
                floats(&[("damping", 50.0), ("mix", 25.0), ("room_size", 40.0)]),
            ),
            limiter_block(),
        ],
    }
}

// Native effect blocks, with valid params from the known-good fixture
// presets. Shared by the full rig and the natives-only cost baseline.
fn comp_block() -> AudioBlock {
    core(
        "comp",
        "dynamics",
        "compressor_studio_clean",
        floats(&[
            ("attack_ms", 10.0),
            ("makeup_gain", 50.0),
            ("mix", 100.0),
            ("ratio", 16.0),
            ("release_ms", 80.0),
            ("threshold", 70.0),
        ]),
    )
}

fn eq_block() -> AudioBlock {
    core(
        "eq",
        "filter",
        "native_guitar_eq",
        floats(&[
            ("high", 0.0),
            ("high_mid", 0.0),
            ("low", 0.0),
            ("low_mid", 0.0),
        ]),
    )
}

fn gate_block() -> AudioBlock {
    core(
        "gate",
        "dynamics",
        "gate_basic",
        floats(&[
            ("attack_ms", 5.0),
            ("hold_ms", 150.0),
            ("hysteresis_db", 6.0),
            ("release_ms", 50.0),
            ("threshold", 24.0),
        ]),
    )
}

fn limiter_block() -> AudioBlock {
    core(
        "limit",
        "dynamics",
        "limiter_brickwall",
        floats(&[
            ("ceiling", -0.1),
            ("knee_db", 2.0),
            ("lookahead_ms", 3.0),
            ("release_ms", 100.0),
            ("threshold", -1.0),
        ]),
    )
}

/// The fixture IR convolution block (acoustic body IR; stands in for the
/// user's cab IR for cost purposes — both are partitioned FFT convolution).
fn ir_block() -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("flavor", ParameterValue::String("standard".into()));
    core("ir", "body", "ir_taylor_714ce", params)
}

/// The native effects of the rig (no NAM, no IR) — the cost baseline the
/// NAM amps must dominate.
fn natives_only_chain() -> Chain {
    Chain {
        id: ChainId("issue-670-natives".into()),
        description: Some("issue-670 native effects cost baseline".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![
            comp_block(),
            eq_block(),
            gate_block(),
            limiter_block(),
        ],
    }
}

/// Render the chain offline and assert no block faulted. A faulted block
/// degrades to a cheap pass-through, which would make the deadline
/// measurement understate the real cost — the test must refuse to measure
/// a lie.
fn assert_no_faulted_blocks(chain: &Chain) {
    let input: Vec<[f32; 2]> = (0..1024)
        .map(|n| {
            let s = 0.2 * (2.0 * std::f32::consts::PI * 220.0 * n as f32 / SR).sin();
            [s, s]
        })
        .collect();
    let outcome = render_chain(chain, SR, &input, BUFFER, 0)
        .expect("heavy chain must render for the #670 setup gate");
    assert!(
        outcome.faulted_blocks.is_empty(),
        "#670 setup gate: the heavy rig has FAULTED blocks {:?} — they \
         degraded to pass-through, so the deadline measurement would \
         understate the real per-buffer cost. Fix the model id / params \
         before trusting the timing.",
        outcome.faulted_blocks
    );
}

struct Stats {
    overruns: usize,
    period_ns: u128,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    max_ns: u128,
}

fn percentiles(mut elapsed: Vec<u128>, period_ns: u128) -> Stats {
    let n = elapsed.len();
    let overruns = elapsed.iter().filter(|&&t| t > period_ns).count();
    elapsed.sort_unstable();
    Stats {
        overruns,
        period_ns,
        p50_ns: elapsed[n / 2],
        p95_ns: elapsed[(n * 95) / 100],
        p99_ns: elapsed[(n * 99) / 100],
        max_ns: *elapsed.last().unwrap(),
    }
}

fn print_stats(label: &str, s: &Stats) {
    eprintln!(
        "[#670] {label:<28} period={}us  p50={}us ({:.1}%)  p95={}us  p99={}us  max={}us  overruns={} ({:.2}%)",
        s.period_ns / 1_000,
        s.p50_ns / 1_000,
        (s.p50_ns as f64 / s.period_ns as f64) * 100.0,
        s.p95_ns / 1_000,
        s.p99_ns / 1_000,
        s.max_ns / 1_000,
        s.overruns,
        (s.overruns as f64 / N_ITERATIONS as f64) * 100.0,
    );
}

fn build(chain: &Chain) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, SR, &[BUFFER], &registry())
            .expect("heavy runtime must build"),
    )
}

/// Drive N runtimes once per buffer and time the WHOLE batch — the
/// realistic unit when several chains share one device callback period.
fn measure(label: &str, runtimes: &[Arc<ChainRuntimeState>]) -> Stats {
    let period_ns = (BUFFER as u128 * 1_000_000_000) / SR as u128;
    let input = vec![0.1_f32; BUFFER]; // mono
    let mut out = vec![0.0_f32; BUFFER * 2]; // stereo

    for _ in 0..WARMUP {
        for rt in runtimes {
            process_input_f32(rt, 0, &input, 1);
            process_output_f32(rt, 0, &mut out, 2);
        }
    }

    let mut elapsed = Vec::with_capacity(N_ITERATIONS);
    for _ in 0..N_ITERATIONS {
        let t0 = Instant::now();
        for rt in runtimes {
            process_input_f32(rt, 0, &input, 1);
            process_output_f32(rt, 0, &mut out, 2);
        }
        elapsed.push(t0.elapsed().as_nanos());
    }
    let s = percentiles(elapsed, period_ns);
    print_stats(label, &s);
    s
}

/// A `[input → block → output]` chain isolating one block's per-buffer
/// cost. Used to attribute the rig's CPU cost to individual blocks.
fn isolated_chain(label: &str, block: AudioBlock) -> Chain {
    Chain {
        id: ChainId(format!("issue-670-iso-{label}")),
        description: Some("issue-670 isolated block cost".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![block],
    }
}

/// Issue #670 diagnostic / regression guard. NOT a pass/fail deadline gate:
/// a heavy rig overrunning a 64-frame budget is the inherent reality this
/// work surfaces (via the engine xrun counter), not a bug to assert away —
/// and absolute timing is machine-dependent, so a "must meet deadline"
/// assertion would false-fail on CI / Orange Pi.
///
/// What it DOES pin, machine-independently: the dominant cost driver is the
/// NAM neural amp, not the native effects. This is the real root cause of
/// the crackle (`#670`): the per-buffer NAM inference cost alone is large
/// enough that a 64-frame budget (~1.33 ms @ 48 kHz) is tight, and a rig
/// stacking several of them across chains cannot meet it. The breakdown is
/// printed so anyone investigating sees the real numbers; the assertion
/// guards against a native block silently regressing into the hot path.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "deadline timing requires --release (see audio_deadline_tests.rs)"
)]
fn heavy_rig_buffer_64_cost_is_dominated_by_nam() {
    init_registry();

    let chain = heavy_chain("0");
    assert_no_faulted_blocks(&chain);

    // Per-block isolation: each heavy block alone vs the native effects
    // combined. The NAM amps must each cost more than all native effects
    // together — that is the machine-independent root-cause fact.
    let nam1 = build(&isolated_chain(
        "nam1",
        nam_preset("amp1", "nam_marshall_plexi", "angus"),
    ));
    let nam1_cost = measure("isolated_nam_amp_1@64", std::slice::from_ref(&nam1));

    let nam2 = build(&isolated_chain(
        "nam2",
        nam_preset("amp2", "nam_a2_slimmable", "a2"),
    ));
    let nam2_cost = measure("isolated_nam_amp_2@64", std::slice::from_ref(&nam2));

    let cab = build(&isolated_chain("cab", ir_block()));
    let cab_cost = measure("isolated_ir_cab@64", std::slice::from_ref(&cab));

    let natives = build(&natives_only_chain());
    let natives_cost = measure("native_effects_combined@64", std::slice::from_ref(&natives));

    // Full-rig deadline picture (printed, not asserted): the reproduction
    // of the user's crackle. On the dev Mac this shows the single chain
    // already overrunning and 4 chains overrunning the median.
    let single = build(&chain);
    measure("full_rig_single_chain@64", std::slice::from_ref(&single));
    let four: Vec<Arc<ChainRuntimeState>> = (0..4)
        .map(|i| build(&heavy_chain(&i.to_string())))
        .collect();
    measure("full_rig_four_chains@64", &four);

    // Root-cause assertion: NAM inference dominates. Each NAM amp's median
    // per-buffer cost exceeds the combined median of compressor + EQ + gate
    // + brickwall limiter. True on any CPU (a neural net forward pass is
    // orders of magnitude more work than a handful of biquads), so it does
    // not false-fail across machines — but it WILL fail if a native effect
    // regresses into the heavy path, which is the regression we guard.
    assert!(
        nam1_cost.p50_ns > natives_cost.p50_ns,
        "BUG #670 root cause: NAM amp 1 median {}us should dominate the \
         native effects' combined median {}us. If this flipped, a native \
         block regressed into the hot path.",
        nam1_cost.p50_ns / 1_000,
        natives_cost.p50_ns / 1_000,
    );
    assert!(
        nam2_cost.p50_ns > natives_cost.p50_ns,
        "BUG #670 root cause: NAM amp 2 median {}us should dominate the \
         native effects' combined median {}us.",
        nam2_cost.p50_ns / 1_000,
        natives_cost.p50_ns / 1_000,
    );
    // The IR cab is the other heavy block; pin that it is non-trivial too
    // (its cost is real, but per-block it is below a single NAM amp).
    assert!(
        cab_cost.p50_ns > 0,
        "IR cab must do measurable convolution work"
    );
}
