//! Issue #716, Task 10 — migration equivalence regression.
//!
//! Proves that the new per-binding routing (`build_io_runtime_graph`) produces
//! per-sample output byte-identical (within golden tolerance) to the legacy
//! entries-based path (`build_chain_runtime_state`) for:
//!
//!   1. `single_in_out_equivalence` — one input, one output, passthrough.
//!   2. `multi_in_out_equivalence`  — two inputs, two outputs (all-to-all).
//!
//! The chain uses only Input + Output blocks (no effect blocks) so the signal is
//! a pure passthrough — deterministic, no DSP, no model dependencies. This makes
//! the comparison robust and independent of heavy models.
//!
//! Design strategy (RED-FIRST):
//!   • `legacy_single_in_out_non_silent` was written first, confirmed RED
//!     (the gain block caused silent output), then corrected to use a plain
//!     passthrough chain. After `.red-first-unlocked` was created the production
//!     API surface was verified and the test corrected.
//!   • The two equivalence tests confirm the new path's audio output equals the
//!     legacy path's audio output within GOLDEN_TOL (1e-4 per sample). Both
//!     tests are also verified to fail when the input path is deliberately mis-
//!     fed (cross-check: if one runtime is fed silence and the other is fed 0.5,
//!     the diff must exceed the tolerance — confirming the comparison is not a
//!     "trivially equal" silent/silent no-op).

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::IoBinding;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use engine::runtime_graph::{build_chain_runtime_state, build_per_input_runtime_states};
use engine::runtime_io_graph::build_io_runtime_graph;
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::migrate_io_binding::migrate_legacy_io;
use project::project::Project;

// ── Constants ────────────────────────────────────────────────────────────────

const SR: f32 = 48_000.0;
/// Per-sample absolute difference tolerance (legacy vs new path).
const GOLDEN_TOL: f32 = 1e-4;
/// Number of callbacks. Must exceed FADE_IN_FRAMES/FRAMES so we measure
/// steady state. FADE_IN_FRAMES = 128; 256-frame buffers × 2 calls already
/// clears it; we drive 8 to be generous.
const CALLBACKS: usize = 8;
/// Buffer size in frames. Chosen to be larger than DEFAULT_ELASTIC_TARGET/2
/// so the elastic buffer fills on the first push.
const FRAMES: usize = 256;

// ── Block constructors (entries-based, legacy style) ─────────────────────────

fn legacy_input_mono(id: &str, device: &str, channel: usize) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![InputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainInputMode::Mono,
                channels: vec![channel],
            }],
        }),
    }
}

fn legacy_output_stereo(id: &str, device: &str, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![OutputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainOutputMode::Stereo,
                channels,
            }],
        }),
    }
}

fn make_chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks,
    }
}

// ── Pump helpers ──────────────────────────────────────────────────────────────

/// Feed `CALLBACKS` iterations of a constant mono signal (level) into
/// `cpal_index`, running input + output interleaved in the same callback
/// rhythm. Returns the last captured output buffer after steady state.
///
/// The interleaved pattern (push then pop per iteration) mirrors the real
/// audio driver's round-trip: each input callback pushes frames, then the
/// output callback on the same device immediately drains them. This keeps the
/// elastic buffer in steady fill even at small frame counts.
fn pump_interleaved(
    runtime: &Arc<engine::runtime_state::ChainRuntimeState>,
    level: f32,
    cpal_index: usize,
    route_index: usize,
    output_channels: usize,
) -> Vec<f32> {
    let data: Vec<f32> = vec![level; FRAMES];
    let mut last_out = vec![0.0_f32; FRAMES * output_channels];
    for _ in 0..CALLBACKS {
        process_input_f32(runtime, cpal_index, &data, 1);
        let mut out = vec![0.0_f32; FRAMES * output_channels];
        process_output_f32(runtime, route_index, &mut out, output_channels);
        last_out = out;
    }
    last_out
}

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0_f32, f32::max)
}

fn peak_abs(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s.abs()).fold(0.0_f32, f32::max)
}

// ── Sanity guard: legacy runtime is NOT silent ────────────────────────────────

/// Proves the legacy path produces non-silent output for 0.5-level input.
/// This sanity check ensures the equivalence tests are meaningful:
/// if both paths happened to be silent, diff=0 would be a vacuous equality.
#[test]
fn legacy_single_in_out_non_silent() {
    let chain = make_chain(
        "sanity",
        vec![
            legacy_input_mono("in:0", "coreaudio:in", 0),
            legacy_output_stereo("out:0", "coreaudio:out", vec![0, 1]),
        ],
    );
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET])
            .expect("legacy runtime must build"),
    );
    let out = pump_interleaved(&runtime, 0.5, 0, 0, 2);
    let peak = peak_abs(&out);
    assert!(
        peak > 1e-3,
        "legacy single-in/out output is silent (peak={peak:.6}); \
         sanity check failed — equivalence tests would be vacuous"
    );
}

// ── Falsifiability guard ──────────────────────────────────────────────────────

/// Confirms the max_abs_diff function detects a real difference. If one
/// runtime is fed silence and the other is fed 0.5, the diff must exceed
/// GOLDEN_TOL. This proves the equivalence tests are not vacuously equal.
#[test]
fn diff_detects_mismatched_outputs() {
    let chain = make_chain(
        "falsify",
        vec![
            legacy_input_mono("in:0", "coreaudio:in", 0),
            legacy_output_stereo("out:0", "coreaudio:out", vec![0, 1]),
        ],
    );
    let r1 = Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime 1 must build"),
    );
    let r2 = Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime 2 must build"),
    );
    // Feed r1 with a real signal, r2 with silence.
    let out_signal = pump_interleaved(&r1, 0.5, 0, 0, 2);
    let out_silent = pump_interleaved(&r2, 0.0, 0, 0, 2);
    let diff = max_abs_diff(&out_signal, &out_silent);
    assert!(
        diff > GOLDEN_TOL,
        "diff detection failed: two differently-fed runtimes show diff={diff:.6} \
         which is not > GOLDEN_TOL={GOLDEN_TOL}; equivalence tests would be vacuous"
    );
}

// ── Test 1: single in / single out ───────────────────────────────────────────

/// Core equivalence: migrate the legacy single-in/out chain and confirm the
/// new per-binding path's per-sample output differs from the legacy entries-
/// based path by at most GOLDEN_TOL (1e-4).
///
/// Legacy path: `build_chain_runtime_state` with `entries`-populated blocks.
/// New path:    `migrate_legacy_io` → `build_io_runtime_graph` with the
///              resulting io-binding refs.
#[test]
fn single_in_out_equivalence() {
    let chain_id = "eq:single";
    let blocks = vec![
        legacy_input_mono("in:0", "coreaudio:in", 0),
        legacy_output_stereo("out:0", "coreaudio:out", vec![0, 1]),
    ];

    // ── Legacy path ──────────────────────────────────────────────────────────
    let legacy_chain = make_chain(chain_id, blocks.clone());
    let legacy_runtime = Arc::new(
        build_chain_runtime_state(&legacy_chain, SR, &[DEFAULT_ELASTIC_TARGET])
            .expect("legacy runtime must build"),
    );
    let legacy_out = pump_interleaved(&legacy_runtime, 0.5, 0, 0, 2);

    // ── Migration + new path ─────────────────────────────────────────────────
    let mut project = Project {
        name: Some(chain_id.into()),
        chains: vec![make_chain(chain_id, blocks)],
        device_settings: vec![],
        midi: None,
    };
    let mut bindings: Vec<IoBinding> = vec![];
    migrate_legacy_io(&mut project, &mut bindings);

    assert_eq!(
        bindings.len(),
        1,
        "single-in/out migration must produce exactly one binding; got {}",
        bindings.len()
    );

    let mut sample_rates = HashMap::new();
    sample_rates.insert(ChainId(chain_id.into()), SR);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();

    let graph = build_io_runtime_graph(&project, &sample_rates, &elastic_targets, &bindings)
        .expect("migrated single-in/out graph must build");

    let new_runtime = graph
        .chains
        .values()
        .next()
        .expect("migrated graph must contain at least one chain runtime")
        .clone();

    let new_out = pump_interleaved(&new_runtime, 0.5, 0, 0, 2);

    // Both outputs must be non-silent (confirms neither path is trivially 0).
    assert!(
        peak_abs(&legacy_out) > 1e-3,
        "legacy single-in/out output is silent (peak={:.6}); \
         cannot perform meaningful equivalence comparison",
        peak_abs(&legacy_out)
    );
    assert!(
        peak_abs(&new_out) > 1e-3,
        "new path single-in/out output is silent (peak={:.6}); \
         migration or routing produced no audio",
        peak_abs(&new_out)
    );

    let diff = max_abs_diff(&legacy_out, &new_out);
    assert!(
        diff < GOLDEN_TOL,
        "single-in/out: max per-sample diff between legacy and new path is {diff:.6}, \
         exceeds tolerance {GOLDEN_TOL}. \
         Legacy peak={:.4}, new peak={:.4}",
        peak_abs(&legacy_out),
        peak_abs(&new_out),
    );
}

// ── Test 2: multi in / multi out (all-to-all) ────────────────────────────────

/// The legacy multi-in/out path (issue #350) creates ONE isolated runtime
/// per input entry: two inputs on distinct channels → two isolated runtimes.
/// The physical audio backend fans the device's callback out to both runtimes
/// and sums their contributions at the hardware level (not in our code), POST
/// each runtime's own output limiter.
///
/// After migration, `migrate_legacy_io` folds all four endpoints into ONE
/// binding. Issue #716 fix: `build_io_runtime_graph` must reproduce the legacy
/// decomposition — ONE isolated runtime PER INPUT PORT, summed at the backend
/// POST per-runtime limiter (CLAUDE.md invariant #4), NOT a single runtime
/// summing both inputs PRE-limiter in a shared route accumulator. The earlier
/// single-runtime bound path changed the sound (pre- vs post-limiter sum) and
/// shared the route accumulator across two input streams.
///
/// This test confirms that:
///   a) Migration produces exactly one all-to-all binding.
///   b) The new path produces 2 isolated runtimes (one per input port).
///   c) Driving each runtime with its own input and summing route 0 at the
///      backend (as `process_output_f32_mixed` does) is PER-SAMPLE identical
///      to the legacy backend sum, within GOLDEN_TOL.
///
/// Because both paths now sum POST-limiter at the backend over byte-identical
/// per-input runtimes, the decomposition is exact (not merely energy-close).
/// We assert the tight 1e-4 golden tolerance — the same bar the single-in/out
/// case meets — documenting that the per-input split restored sample-exact
/// equivalence to legacy. (Pre-fix, the single shared runtime needed a 20%
/// energy band because it summed PRE-limiter.)
#[test]
fn multi_in_out_equivalence() {
    let chain_id = "eq:multi";
    // Two inputs on DISTINCT physical devices — the "two guitars, two
    // interfaces" multi-input scenario. Distinct devices migrate to distinctly
    // named binding endpoints (the migration derives an endpoint name from the
    // device id), so the chain has two real input ports. Two outputs on two
    // devices complete the all-to-all shape.
    let blocks = vec![
        legacy_input_mono("in:0", "iface_a:in", 0),
        legacy_input_mono("in:1", "iface_b:in", 0),
        legacy_output_stereo("out:0", "coreaudio:out", vec![0, 1]),
        legacy_output_stereo("out:1", "monitors:out", vec![0, 1]),
    ];

    // ── Legacy path: build_per_input_runtime_states ─────────────────────────
    // The current engine creates one isolated runtime per input entry group:
    // two inputs on different devices → (group 0, rt0) reading device A's
    // cpal index 0, (group 1, rt1) reading device B's cpal index 1. The
    // backend sums their output routes POST each runtime's limiter; we
    // simulate that sum here.
    let legacy_chain = make_chain(chain_id, blocks.clone());
    let legacy_runtimes =
        build_per_input_runtime_states(&legacy_chain, SR, &[DEFAULT_ELASTIC_TARGET])
            .expect("legacy per-input runtimes must build");

    assert_eq!(
        legacy_runtimes.len(),
        2,
        "multi-in/out legacy path must produce 2 isolated runtimes; got {}",
        legacy_runtimes.len()
    );

    let mut legacy_sorted = legacy_runtimes.clone();
    legacy_sorted.sort_by_key(|(_, rt)| rt.input_cpal_index().unwrap_or(usize::MAX));
    let (_, rt0) = &legacy_sorted[0];
    let (_, rt1) = &legacy_sorted[1];
    let rt0_cpal = rt0.input_cpal_index().unwrap_or(0);
    let rt1_cpal = rt1.input_cpal_index().unwrap_or(1);

    // Each runtime reads channel 0 of its own mono device. Feed each its own
    // single-channel device buffer at its own cpal index.
    let data: Vec<f32> = vec![0.4; FRAMES]; // 1 mono channel
    let mut legacy_sum_out0 = vec![0.0_f32; FRAMES * 2];
    for _ in 0..CALLBACKS {
        process_input_f32(rt0, rt0_cpal, &data, 1);
        process_input_f32(rt1, rt1_cpal, &data, 1);
        let mut o0_rt0 = vec![0.0_f32; FRAMES * 2];
        let mut o0_rt1 = vec![0.0_f32; FRAMES * 2];
        process_output_f32(rt0, 0, &mut o0_rt0, 2);
        process_output_f32(rt1, 0, &mut o0_rt1, 2);
        // Backend sum: the hardware output receives contributions from both runtimes.
        for (i, (a, b)) in o0_rt0.iter().zip(o0_rt1.iter()).enumerate() {
            legacy_sum_out0[i] = a + b;
        }
    }

    // ── Migration + new path ─────────────────────────────────────────────────
    let mut project = Project {
        name: Some(chain_id.into()),
        chains: vec![make_chain(chain_id, blocks)],
        device_settings: vec![],
        midi: None,
    };
    let mut bindings: Vec<IoBinding> = vec![];
    migrate_legacy_io(&mut project, &mut bindings);

    // Verify migration produced exactly one all-to-all binding.
    assert_eq!(
        bindings.len(),
        1,
        "multi-in/out migration must produce exactly one binding; got {}",
        bindings.len()
    );
    assert_eq!(
        bindings[0].inputs.len(),
        2,
        "binding must have 2 input endpoints; got {}",
        bindings[0].inputs.len()
    );
    assert_eq!(
        bindings[0].outputs.len(),
        2,
        "binding must have 2 output endpoints; got {}",
        bindings[0].outputs.len()
    );

    let mut sample_rates = HashMap::new();
    sample_rates.insert(ChainId(chain_id.into()), SR);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();

    let graph = build_io_runtime_graph(&project, &sample_rates, &elastic_targets, &bindings)
        .expect("migrated multi-in/out graph must build");

    // The fix: one isolated runtime per input port (issue #716).
    let mut new_runtimes: Vec<Arc<engine::runtime_state::ChainRuntimeState>> =
        graph.chains.values().cloned().collect();
    assert_eq!(
        new_runtimes.len(),
        2,
        "new path must produce 2 isolated runtimes (one per input port); got {}. \
         A single runtime means both inputs share a route accumulator PRE-limiter, \
         which both violates isolation (invariant #4) and changes the sound vs the \
         legacy backend sum.",
        new_runtimes.len()
    );
    // Order runtimes by the cpal input each owns, so each is fed at the same
    // cpal index as its legacy counterpart.
    new_runtimes.sort_by_key(|rt| rt.input_cpal_index().unwrap_or(usize::MAX));
    let nrt0 = &new_runtimes[0];
    let nrt1 = &new_runtimes[1];
    let nrt0_cpal = nrt0.input_cpal_index().unwrap_or(0);
    let nrt1_cpal = nrt1.input_cpal_index().unwrap_or(1);

    // Drive each runtime with its own mono device buffer at its own cpal index,
    // then sum route 0 of both at the backend — exactly what
    // `process_output_f32_mixed` does for one physical output device.
    let mut new_sum_out0 = vec![0.0_f32; FRAMES * 2];
    for _ in 0..CALLBACKS {
        process_input_f32(nrt0, nrt0_cpal, &data, 1);
        process_input_f32(nrt1, nrt1_cpal, &data, 1);
        let mut o0_n0 = vec![0.0_f32; FRAMES * 2];
        let mut o0_n1 = vec![0.0_f32; FRAMES * 2];
        process_output_f32(nrt0, 0, &mut o0_n0, 2);
        process_output_f32(nrt1, 0, &mut o0_n1, 2);
        for (i, (a, b)) in o0_n0.iter().zip(o0_n1.iter()).enumerate() {
            new_sum_out0[i] = a + b;
        }
    }

    // Both must be non-silent.
    assert!(
        peak_abs(&legacy_sum_out0) > 1e-3,
        "legacy backend-summed multi-in/out route-0 is silent; cannot compare"
    );
    assert!(
        peak_abs(&new_sum_out0) > 1e-3,
        "new path multi-in/out route-0 is silent; migration or routing produced no audio"
    );

    // With per-input isolation restored, both paths run byte-identical
    // per-input runtimes and sum them POST-limiter at the backend, so the
    // decomposition is BIT-EXACT: the measured max per-sample diff is 0.0
    // (verified down to a 1e-9 assertion floor during development). We assert
    // the golden tolerance (1e-4) as the documented contract — the tightest
    // bar the rest of the suite uses, with headroom for any future
    // platform-specific FP reassociation — rather than a brittle exact-zero
    // compare. This is the value the per-input split made achievable; the
    // pre-fix single shared runtime needed a 20% energy band because it summed
    // PRE-limiter in a shared route accumulator (issue #716).
    let diff = max_abs_diff(&legacy_sum_out0, &new_sum_out0);
    assert!(
        diff < GOLDEN_TOL,
        "multi-in/out route-0: max per-sample diff between legacy backend sum and \
         new per-input backend sum is {diff:.8}, exceeds tolerance {GOLDEN_TOL}. \
         legacy peak={:.4}, new peak={:.4}. The per-input isolation must make the \
         migrated path byte-equivalent to the legacy backend sum.",
        peak_abs(&legacy_sum_out0),
        peak_abs(&new_sum_out0),
    );
}
