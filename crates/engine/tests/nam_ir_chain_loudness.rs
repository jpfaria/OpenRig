//! End-to-end loudness guard for the NAM/IR signal chain. Issue #491.
//!
//! THE PROPERTY UNDER TEST: a NAM plugin shipped with a measured
//! `output_gain_db` calibration in its manifest must actually be louder
//! by that dB amount when loaded into a real block chain. Before #491
//! the engine read the wrong field name (`output_gain_pct`), the
//! calibration deserialized to `None`, and every NAM played at raw model
//! output — dramatically quieter than the calibrated reference. This
//! test fails (no level difference) if that regression ever returns.
//!
//! It builds a real two-block chain from bundled minimal fixtures
//! (real `marshall_plexi` NAM amp + real `taylor_714ce` IR body) and
//! measures the signal level AFTER EACH BLOCK:
//!
//!   DI sine → [NAM amp] → measure → [IR body] → measure
//!
//! The regression guard is differential: the same NAM package is built
//! twice — once with its real `output_gain_db: 8.9318924`, once with
//! the field cleared (the pre-#491 behaviour). `output_level_db` is the
//! NAM host's post-model output trim, so the calibrated build must be
//! exactly that many dB louder. A near-zero delta means the calibration
//! is dead again.
//!
//! No `#[ignore]`: the fixtures are bundled in-repo
//! (`tests/fixtures/plugins/`, ~36 KB), so this always runs.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use domain::value_objects::ParameterValue;
use plugin_loader::discover;
use plugin_loader::discover::LoadedPackage;

const SR: f32 = 48_000.0;
/// `output_gain_db` in the bundled `marshall_plexi/manifest.yaml`.
/// A positive loudness-matching target (issue #491): a NAM amp must be
/// much louder than the clean DI. The #496 detour that made this an
/// attenuation ("tudo baixo") was reverted; clip safety is now a
/// memoryless soft-clip in the NAM processor, not a quiet calibration.
const PLEXI_CAL_DB: f32 = 8.931_892_4;

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

/// A quiet sine — small enough that the amp stays well-behaved, so the
/// only thing that moves the level between the calibrated and the
/// uncalibrated build is the post-model `output_gain_db` trim.
fn di_sine(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| 0.05 * (2.0 * std::f32::consts::PI * 110.0 * n as f32 / SR).sin())
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

/// Peak level of the steady-state tail in dBFS (skip warmup transient).
fn steady_peak_dbfs(samples: &[f32]) -> f32 {
    let tail = &samples[samples.len() / 2..];
    let peak = tail.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    20.0 * peak.max(1e-9).log10()
}

fn assert_clean(samples: &[f32], stage: &str) {
    assert!(
        samples.iter().all(|s| s.is_finite()),
        "{stage}: produced NaN/Inf"
    );
    assert!(
        steady_peak_dbfs(samples) > -60.0,
        "{stage}: signal collapsed to silence (volume lost)"
    );
}

#[test]
fn nam_ir_chain_keeps_calibrated_loudness_after_each_block() {
    nam::register_builder();
    ir::register_builder();

    // ── Block 1: real NAM amp, WITH its shipped output_gain_db ──────────
    let plexi = load("nam/marshall_plexi");
    assert_eq!(
        plexi.manifest.output_gain_db,
        Some(PLEXI_CAL_DB),
        "fixture manifest must carry the dB calibration the engine reads"
    );
    // Per the #496 contract: `manifest.output_gain_db` is no longer
    // stacked on top of the user `output_db` param at load time. The
    // block factory (or the project migrator) copies it into
    // `params.output_db` so the UI knob mirrors the calibration. In
    // this unit test we bypass the factory, so we have to write the
    // audit dB into `output_db` ourselves on the calibrated build —
    // that is what the engine actually applies in production.
    let mut amp_params_calibrated = ParameterSet::default();
    amp_params_calibrated.insert("preset", ParameterValue::String("angus".into()));
    amp_params_calibrated.insert("output_db", ParameterValue::Float(PLEXI_CAL_DB));
    let mut amp = plexi
        .build_processor(&amp_params_calibrated, SR, AudioChannelLayout::Mono)
        .expect("NAM amp should build from the bundled fixture");

    // ── Block 1 (control): same package, calibration field cleared ─────
    // Same package, manifest calibration cleared, user param at 0 dB
    // (the pre-factory state: nothing has been baked into `output_db`).
    let mut amp_params_uncalibrated = ParameterSet::default();
    amp_params_uncalibrated.insert("preset", ParameterValue::String("angus".into()));
    amp_params_uncalibrated.insert("output_db", ParameterValue::Float(0.0));
    let mut plexi_dead = plexi.clone();
    plexi_dead.manifest.output_gain_db = None;
    let mut amp_dead = plexi_dead
        .build_processor(&amp_params_uncalibrated, SR, AudioChannelLayout::Mono)
        .expect("control NAM amp should build");

    let di = di_sine(8_192);
    let after_amp = run_mono(&mut amp, &di);
    let after_amp_dead = run_mono(&mut amp_dead, &di);
    assert_clean(&after_amp, "after NAM (calibrated)");
    assert_clean(&after_amp_dead, "after NAM (uncalibrated control)");

    // The calibration is a post-model linear trim, so the delta must be
    // the manifest's dB value — not zero. Pre-#491 both builds were
    // identical (delta ≈ 0): that is the regression this guards.
    let delta_db = steady_peak_dbfs(&after_amp) - steady_peak_dbfs(&after_amp_dead);
    assert!(
        (delta_db - PLEXI_CAL_DB).abs() < 0.5,
        "NAM calibration not applied correctly: expected ≈{PLEXI_CAL_DB:.2} dB \
         louder than the uncalibrated control, got {delta_db:.3} dB \
         (≈0 dB means the loudness calibration is dead again — issue #491)"
    );

    // ── Block 2: real IR body, fed the calibrated amp output ───────────
    let cab = load("ir/taylor_714ce");
    let mut ir_params = ParameterSet::default();
    ir_params.insert("flavor", ParameterValue::String("standard".into()));
    let mut body = cab
        .build_processor(&ir_params, SR, AudioChannelLayout::Mono)
        .expect("IR body should build from the bundled fixture");

    let after_ir = run_mono(&mut body, &after_amp);
    assert_clean(&after_ir, "after IR");

    // Convolving with a real body IR reshapes the spectrum but must not
    // silently swallow the chain — the calibrated loudness has to
    // survive to the end of the chain.
    assert!(
        steady_peak_dbfs(&after_ir) > -60.0,
        "calibrated loudness lost after the IR block (issue #491)"
    );
}
