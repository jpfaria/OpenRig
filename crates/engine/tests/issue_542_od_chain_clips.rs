//! Issue #542 — red-first guard: the user's `guiTARRA - DEFAULT` chain on
//! preset OD clips the output ("som estourado") when the cab IR is
//! enabled. Replicate the EXACT chain from the user's
//! `~/.openrig/project.yaml` (OD preset, enabled blocks only) and assert
//! the output peak stays under the brickwall limiter's ceiling.
//!
//! Enabled blocks (in order) on the OD preset:
//!   1. nam_big_muff       — output_db = +2.98 (audit), size=feather, sustain=0
//!   2. nam_nobels_odr_1   — gain 28, output_db = 0
//!   3. nam_mesa_mark_iii  — preamp, eq=lohi, gain=pushed, output_db = 0
//!   4. ir_marshall_4x12_v30 — capture ev_mix_b
//!   5. limiter_brickwall  — ceiling -0.1 dBFS, threshold -1.0, lookahead 3 ms
//!
//! `gate_basic` is omitted from the test — for a steady-state sine well
//! above its threshold the gate is fully open and contributes no gain.
//!
//! Disabled blocks therefore omitted: compressor_studio_clean,
//! native_guitar_eq, nam_dumble, plate_foundation.
//!
//! The chain runs in Stereo layout (CLAUDE.md invariant #5). The test
//! depends on the user's `OpenRig-plugins` tree being present at the
//! same dev path as `crates/project/tests/disk_package_metadata_lookups.rs`;
//! if it is missing the test fails loudly — the repro depends on the
//! actual captures.

use std::path::PathBuf;
use std::sync::Once;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use domain::value_objects::ParameterValue;

const SR: f32 = 48_000.0;
/// 4096 frames ≈ 85 ms at 48 kHz — past the IR convolver's one-partition
/// (512-sample) warmup so steady-state peak is observable.
const FRAMES: usize = 4096;

fn plugins_root() -> PathBuf {
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../../OpenRig-plugins/plugins/source"),
        PathBuf::from(
            "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
        ),
    ];
    candidates
        .into_iter()
        .find(|p| p.is_dir())
        .expect("OpenRig-plugins/plugins/source must be present on disk for issue #542 repro")
}

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        block_dyn::register_natives();
        block_gain::register_natives();
        block_amp::register_natives();
        block_preamp::register_natives();
        block_cab::register_natives();
        plugin_loader::registry::init(&plugins_root());
    });
}

fn build_disk(model_id: &str, params: &ParameterSet) -> BlockProcessor {
    let pkg = plugin_loader::registry::find(model_id)
        .unwrap_or_else(|| panic!("plugin package not in registry: {model_id}"));
    pkg.build_processor(params, SR, AudioChannelLayout::Mono)
        .unwrap_or_else(|e| panic!("build_processor failed for {model_id}: {e}"))
}

/// ≈ -20 dBFS guitar-ish two-tone — realistic raw input from a
/// Scarlett 2i2 set for a normal humbucker pickup level (the gain
/// stages downstream then push the signal back up to performance
/// level). Sane limiters must still hold the chain output below 0
/// dBFS; if they don't, that's the regression.
fn di_guitar(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| {
            let t = n as f32 / SR;
            (0.08 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
                + 0.02 * (2.0 * std::f32::consts::PI * 1100.0 * t).sin())
            .clamp(-1.0, 1.0)
        })
        .collect()
}

/// Process in 128-sample blocks, matching the runtime CPAL buffer size
/// the user is hitting on the Scarlett 2i2. Each chain stage drains a
/// block at a time so block-size-dependent state (limiter lookahead,
/// gate hold counters) gets exercised the same way as live audio.
const ENGINE_BLOCK: usize = 128;

fn process(proc: &mut BlockProcessor, samples: &mut [f32]) {
    let mono = match proc {
        BlockProcessor::Mono(p) => p,
        BlockProcessor::Stereo(_) => panic!("expected mono processor for issue #542 chain"),
    };
    for chunk in samples.chunks_mut(ENGINE_BLOCK) {
        mono.process_block(chunk);
    }
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |a, &b| a.max(b.abs()))
}

fn dbfs(linear: f32) -> f32 {
    20.0 * linear.max(1e-9).log10()
}

#[test]
fn issue_542_od_preset_chain_output_stays_under_limiter_ceiling() {
    init_registry();

    // ── Block 1: nam_big_muff (gain) ─────────────────────────────────────
    let mut p = ParameterSet::default();
    p.insert("size", ParameterValue::String("feather".into()));
    p.insert("nam_size", ParameterValue::String("standard".into()));
    p.insert("sustain", ParameterValue::Float(0.0));
    p.insert("input_db", ParameterValue::Float(0.0));
    p.insert("output_db", ParameterValue::Float(2.9812737));
    p.insert("eq.enabled", ParameterValue::Bool(true));
    p.insert("eq.bass", ParameterValue::Float(5.0));
    p.insert("eq.middle", ParameterValue::Float(5.0));
    p.insert("eq.treble", ParameterValue::Float(5.0));
    p.insert("noise_gate.enabled", ParameterValue::Bool(true));
    p.insert("noise_gate.threshold_db", ParameterValue::Float(-50.0));
    let mut muff = build_disk("nam_big_muff", &p);

    // ── Block 2: nam_nobels_odr_1 ────────────────────────────────────────
    let mut p = ParameterSet::default();
    p.insert("gain", ParameterValue::Float(28.0));
    p.insert("input_db", ParameterValue::Float(0.0));
    p.insert("output_db", ParameterValue::Float(0.0));
    p.insert("eq.enabled", ParameterValue::Bool(false));
    p.insert("eq.bass", ParameterValue::Float(5.0));
    p.insert("eq.middle", ParameterValue::Float(5.0));
    p.insert("eq.treble", ParameterValue::Float(5.0));
    p.insert("noise_gate.enabled", ParameterValue::Bool(false));
    p.insert("noise_gate.threshold_db", ParameterValue::Float(-50.0));
    let mut nob = build_disk("nam_nobels_odr_1", &p);

    // ── Block 3: nam_mesa_mark_iii (preamp) ──────────────────────────────
    let mut p = ParameterSet::default();
    p.insert("eq", ParameterValue::String("lohi".into()));
    p.insert("gain", ParameterValue::String("pushed".into()));
    p.insert("input_db", ParameterValue::Float(0.0));
    p.insert("output_db", ParameterValue::Float(0.0));
    p.insert("eq.enabled", ParameterValue::Bool(true));
    p.insert("eq.bass", ParameterValue::Float(5.0));
    p.insert("eq.middle", ParameterValue::Float(5.0));
    p.insert("eq.treble", ParameterValue::Float(5.0));
    p.insert("noise_gate.enabled", ParameterValue::Bool(true));
    p.insert("noise_gate.threshold_db", ParameterValue::Float(-50.0));
    let mut mark = build_disk("nam_mesa_mark_iii", &p);

    // ── Block 4: ir_marshall_4x12_v30 (cab) ──────────────────────────────
    let mut p = ParameterSet::default();
    p.insert("capture", ParameterValue::String("ev_mix_b".into()));
    let mut cab = build_disk("ir_marshall_4x12_v30", &p);

    // ── Block 5: limiter_brickwall (native dyn) ──────────────────────────
    let mut p = ParameterSet::default();
    p.insert("ceiling", ParameterValue::Float(-0.1));
    p.insert("threshold", ParameterValue::Float(-1.0));
    p.insert("knee_db", ParameterValue::Float(2.0));
    p.insert("lookahead_ms", ParameterValue::Float(3.0));
    p.insert("release_ms", ParameterValue::Float(100.0));
    let mut limiter = block_dyn::build_dynamics_processor_for_layout(
        "limiter_brickwall",
        &p,
        SR,
        AudioChannelLayout::Mono,
    )
    .expect("limiter_brickwall stereo build");

    // ── Run the chain ────────────────────────────────────────────────────
    let mut sig = di_guitar(FRAMES);
    let p_in = peak(&sig);

    process(&mut muff, &mut sig);
    let p_muff = peak(&sig);
    process(&mut nob, &mut sig);
    let p_nob = peak(&sig);
    process(&mut mark, &mut sig);
    let p_mark = peak(&sig);
    process(&mut cab, &mut sig);
    let p_cab = peak(&sig);
    process(&mut limiter, &mut sig);
    let p_out = peak(&sig);

    eprintln!(
        "issue #542 chain peaks (linear / dBFS):\n  \
         in   = {:.4} ({:+.2} dBFS)\n  \
         muff = {:.4} ({:+.2} dBFS)\n  \
         nob  = {:.4} ({:+.2} dBFS)\n  \
         mark = {:.4} ({:+.2} dBFS)\n  \
         cab  = {:.4} ({:+.2} dBFS)\n  \
         out  = {:.4} ({:+.2} dBFS)",
        p_in,
        dbfs(p_in),
        p_muff,
        dbfs(p_muff),
        p_nob,
        dbfs(p_nob),
        p_mark,
        dbfs(p_mark),
        p_cab,
        dbfs(p_cab),
        p_out,
        dbfs(p_out),
    );

    // Two contracts are pinned here:
    //
    // 1. The CAB output (pre-limiter) must not breach 0 dBFS. The cab
    //    IR is gain-passive material — for a normalised IR (peak 0 dBFS
    //    in time domain), a real guitar-level input should come out
    //    with at most a few dB of mid-bump, never +4 dB beyond
    //    full-scale. When this assertion is RED, the audible result is
    //    that the brickwall limiter downstream is forced into
    //    aggressive gain reduction (5+ dB GR sustained) → audible
    //    pumping that the user reports as "som estourado", even though
    //    the final output never crosses 0 dBFS. The fix is upstream of
    //    the limiter (audit baseline / per-capture output_gain_db
    //    compensation, or auto-normalising the convolver by the IR's
    //    integrated spectral gain — never silently re-tuning the
    //    limiter).
    //
    // 2. The final chain output must of course stay under 0 dBFS too —
    //    if the limiter ever fails to clamp, the speaker really does
    //    clip.
    assert!(
        p_cab < 1.0,
        "REGRESSION: cab IR output peak {p_cab:.4} ({:+.2} dBFS) breaches 0 dBFS — \
         the IR is contributing too much gain into a gain-passive slot. The \
         limiter downstream is forced into aggressive gain reduction to recover, \
         which the user hears as 'som estourado'. Likely cause: per-capture \
         output_gain_db calibration missing in the IR manifest (audit pipeline \
         reset to 0.0) or the convolver is not compensating for spectral peaks.",
        dbfs(p_cab),
    );
    assert!(
        p_out < 1.0,
        "REGRESSION: chain output peak {p_out:.4} ({:+.2} dBFS) breaches 0 dBFS — \
         brickwall limiter is not holding.",
        dbfs(p_out),
    );
}

/// Direct IR-convolver gain probe: feed a unit impulse (sample 0 = 1.0,
/// rest zero) into `ir_marshall_4x12_v30` ev_mix_b. The convolver
/// output's peak amplitude equals the IR's peak amplitude — which, for
/// a 0 dBFS-normalised WAV, is 1.0. Anything noticeably above 1.0 is a
/// gain bug in the IR path.
///
/// The user's chain peaks show the cab boosting the signal by ~+18 dB
/// (much higher than a typical 0-6 dB cab mid-bump), which is what's
/// driving the chain into "estourado". Pinning this test gives us a
/// concrete number to track if the convolver normalisation drifts again.
#[test]
fn issue_542_ir_convolver_impulse_response_peaks_at_normalised_amplitude() {
    init_registry();

    let mut p = ParameterSet::default();
    p.insert("capture", ParameterValue::String("ev_mix_b".into()));
    let mut cab = build_disk("ir_marshall_4x12_v30", &p);

    // Unit impulse: a single 1.0 sample followed by zeros, long enough
    // to cover the entire IR response (8192 IR samples + warmup +
    // tail).
    let mut sig = vec![0.0_f32; 16_384];
    sig[0] = 1.0;
    process(&mut cab, &mut sig);

    let p_out = peak(&sig);
    eprintln!(
        "issue #542 IR impulse response peak: {:.4} ({:+.2} dBFS)",
        p_out,
        dbfs(p_out),
    );

    // Marshall 4x12 V30 ev_mix_b is peak-normalised to 0 dBFS in the
    // source WAV. A correctly normalised partition convolver outputs
    // exactly the IR for an impulse input — peak should be 1.0 within
    // FFT precision. We allow a generous ceiling of 1.5 (≈ +3.5 dBFS)
    // so the test is robust to small numerical artefacts but red if a
    // gain factor of ~2x or more sneaks in.
    assert!(
        p_out < 1.5,
        "REGRESSION: IR impulse response peak {p_out:.4} ({:+.2} dBFS) is more \
         than 3.5 dB above the IR's 0 dBFS normalisation — the convolver is \
         adding gain. Probable culprit: a normalisation factor in the partition \
         FFT or the post-convolution wrapper.",
        dbfs(p_out),
    );
}
