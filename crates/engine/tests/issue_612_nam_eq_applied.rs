//! Issue #612 — NAM plugin EQ params (`eq.bass` / `eq.middle` / `eq.treble`)
//! must actually shape the output, not just be parsed into `NamPluginParams`
//! and ignored. The noise gate had exactly this bug until #496 ("parsed but
//! never applied"); the EQ is suspected of the same.
//!
//! Objective gate (numbers, not ears): run the SAME rich DI through the real
//! bundled NAM model twice — once bass-heavy, once treble-heavy — with the EQ
//! enabled. If the EQ is wired, the two outputs differ. If it is a no-op, they
//! are byte-identical.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use domain::value_objects::ParameterValue;
use plugin_loader::discover;
use plugin_loader::discover::LoadedPackage;

const SR: f32 = 48_000.0;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins")
}

fn load(rel_dir: &str) -> LoadedPackage {
    let root = fixtures_root();
    let target = root.join(rel_dir);
    discover::discover(&root)
        .expect("discover fixture root")
        .into_iter()
        .filter_map(Result::ok)
        .find(|p| p.root == target)
        .unwrap_or_else(|| panic!("fixture package not found: {rel_dir}"))
}

fn run_mono(proc: &mut BlockProcessor, input: &[f32]) -> Vec<f32> {
    let mut buf = input.to_vec();
    match proc {
        BlockProcessor::Mono(m) => m.process_block(&mut buf),
        BlockProcessor::Stereo(_) => panic!("expected mono"),
    }
    buf
}

/// Rich DI with low + mid + high content so a tone stack has something to
/// shape across the spectrum (the model also adds harmonics).
fn rich_di(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| {
            let t = n as f32 / SR;
            let tau = 2.0 * std::f32::consts::PI;
            0.15 * (tau * 110.0 * t).sin()
                + 0.10 * (tau * 1_500.0 * t).sin()
                + 0.08 * (tau * 6_000.0 * t).sin()
        })
        .collect()
}

fn process_with_params(extra: &[(&str, ParameterValue)]) -> Vec<f32> {
    nam::register_builder();
    let pkg = load("nam/marshall_plexi");
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String("angus".into()));
    for (k, v) in extra {
        params.insert(*k, v.clone());
    }
    let mut amp = pkg
        .build_processor(&params, SR, AudioChannelLayout::Mono)
        .expect("build NAM");
    run_mono(&mut amp, &rich_di(8_192))
}

fn process_with_eq(bass: f32, middle: f32, treble: f32) -> Vec<f32> {
    process_with_params(&[
        ("eq.enabled", ParameterValue::Bool(true)),
        ("eq.bass", ParameterValue::Float(bass)),
        ("eq.middle", ParameterValue::Float(middle)),
        ("eq.treble", ParameterValue::Float(treble)),
    ])
}

#[test]
fn nam_eq_bass_vs_treble_changes_the_output() {
    let bass_heavy = process_with_eq(10.0, 5.0, 0.0);
    let treble_heavy = process_with_eq(0.0, 5.0, 10.0);

    assert_eq!(bass_heavy.len(), treble_heavy.len());
    let max_diff = bass_heavy
        .iter()
        .zip(&treble_heavy)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);

    assert!(
        max_diff > 1e-4,
        "NAM EQ produced identical output for bass-heavy vs treble-heavy \
         settings (max sample diff {max_diff:.2e}) — eq.bass/eq.treble are \
         parsed but never applied to the signal (issue #612)"
    );
}

#[test]
fn nam_eq_flat_center_is_transparent() {
    // Invariant: the default tone (5/5/5, enabled) must be byte-identical
    // to the EQ disabled — wiring the tone stack must NOT color the
    // existing sound at the center detent (no golden-sample drift).
    let flat_center = process_with_eq(5.0, 5.0, 5.0);
    let eq_off = process_with_params(&[("eq.enabled", ParameterValue::Bool(false))]);

    assert_eq!(
        flat_center, eq_off,
        "flat EQ (5/5/5) must be byte-identical to EQ disabled — the center \
         detent must be transparent so existing tones don't change (issue #612)"
    );
}
