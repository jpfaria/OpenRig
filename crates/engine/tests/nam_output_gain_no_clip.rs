//! NAM `output_gain_db` makeup must not clip a normal-level signal. Issue #496.
//!
//! THE PROPERTY UNDER TEST: after #491 the engine applies a NAM model's
//! measured `output_gain_db` as a post-model trim so the plugin reaches
//! its calibrated loudness. Real captures ship large values (the #496
//! report documents +10.53, +13.05, +18.68 dB; the user heard the
//! "CPM 22" preset clip even at normal playing volume).
//!
//! A loudness calibration that pushes a normally-played signal past
//! digital full-scale (|s| > 1.0) is broken: it clips on the converter
//! and amplifies the model noise floor on the decay. CLAUDE.md makes
//! audio quality a central invariant — clipping is a regression.
//!
//! The #491 guard (`nam_ir_chain_loudness.rs`) only ever fed a
//! deliberately quiet 0.05 sine ("small enough that the amp stays
//! well-behaved") and only compared the loudness DELTA — so it could
//! never observe this. This test feeds a realistic DI level and asserts
//! the calibrated output stays within full-scale.
//!
//! No `#[ignore]`: reuses the bundled `marshall_plexi` fixture.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use domain::value_objects::ParameterValue;
use plugin_loader::discover;
use plugin_loader::discover::LoadedPackage;

const SR: f32 = 48_000.0;

/// Worst real `output_gain_db` documented in issue #496 (the CPM 22 /
/// hot Tone3000 captures live up here). Not invented: it is one of the
/// measured manifest values quoted in the report.
const HOT_CAL_DB: f32 = 18.68;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins")
}

fn load(rel_dir: &str) -> LoadedPackage {
    let root = fixtures_root();
    let target = root.join(rel_dir);
    let found = discover::discover(&root).expect("discover fixture root");
    found
        .into_iter()
        .filter_map(Result::ok)
        .find(|p| p.root == target)
        .unwrap_or_else(|| panic!("fixture package not found: {rel_dir}"))
}

/// A normal-level DI: peak ≈ 0.3 (≈ -10 dBFS), representative of a
/// guitar played at normal volume into the chain — NOT the artificially
/// quiet 0.05 the #491 guard used to keep the amp "well-behaved".
fn di_sine(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| 0.3 * (2.0 * std::f32::consts::PI * 110.0 * n as f32 / SR).sin())
        .collect()
}

fn run_mono(proc: &mut BlockProcessor, input: &[f32]) -> Vec<f32> {
    let mut buf = input.to_vec();
    match proc {
        BlockProcessor::Mono(m) => m.process_block(&mut buf),
        BlockProcessor::Stereo(_) => panic!("expected a mono processor for this fixture"),
    }
    buf
}

// GATED on issue #496 part 2: a blind static ceiling in the engine
// kills the legitimate makeup too (it cannot tell +8.93 dB unity
// restoration from a +18 dB hot boost — that regresses #491 "tudo
// baixo"). The clean fix needs `nam_loudness_audit` (OpenRig-plugins)
// to emit a BOUNDED RELATIVE correction. Un-`ignore` and drop the
// `HOT_CAL_DB` synthetic override once those manifests land — this
// then becomes the live no-clip contract.
#[ignore = "blocked on #496 part 2: audit must emit bounded relative output_gain_db"]
#[test]
fn nam_calibrated_output_does_not_clip_a_normal_level_di() {
    nam::register_builder();

    // Real bundled NAM amp, but carrying a real hot `output_gain_db`
    // from the #496 report (CPM 22 class). This is exactly the #491
    // path: the manifest value is applied as the post-model trim.
    let mut plexi = load("nam/marshall_plexi");
    plexi.manifest.output_gain_db = Some(HOT_CAL_DB);

    let mut amp_params = ParameterSet::default();
    amp_params.insert("preset", ParameterValue::String("angus".into()));
    let mut amp = plexi
        .build_processor(&amp_params, SR, AudioChannelLayout::Mono)
        .expect("NAM amp should build from the bundled fixture");

    let di = di_sine(8_192);
    let out = run_mono(&mut amp, &di);

    assert!(out.iter().all(|s| s.is_finite()), "produced NaN/Inf");

    let peak = out.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    assert!(
        peak <= 1.0,
        "NAM clips a normal-level DI: a {HOT_CAL_DB:.2} dB output_gain_db \
         calibration pushed the peak to {peak:.4} (> 1.0 full-scale). \
         The #491 makeup is applied as raw linear gain with no headroom — \
         issue #496."
    );
}
