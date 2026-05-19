//! NAM `output_gain_db` makeup must not clip a normal-level signal. Issue #496.
//!
//! THE PROPERTY UNDER TEST: the engine applies a NAM model's manifest
//! `output_gain_db` as a post-model linear trim (#491). When that value
//! was an absolute hot loudness target (the #496 report documented
//! +10.53 / +13.05 / +18.68 dB; the user heard the "CPM 22" preset clip
//! at normal playing volume) the post-model gain had no headroom: a
//! normal DI peaked ~3x past full-scale and the same gain amplified the
//! model noise floor on the decay.
//!
//! Resolution (#496 part 2): `nam_loudness_audit` (OpenRig-plugins) now
//! emits a BOUNDED RELATIVE correction instead of a hot target — the
//! bundled `marshall_plexi` fixture mirrors its real re-audited source
//! value (tone3000 #2717: +8.93 → -13.56 dB). With those values the
//! unchanged post-model trim must keep a normally-played signal inside
//! digital full-scale. This is the live regression guard: if a hot
//! absolute value is ever re-introduced into a manifest, building it
//! and running a normal DI clips here.
//!
//! No `#[ignore]`: reuses the bundled `marshall_plexi` fixture.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use domain::value_objects::ParameterValue;
use plugin_loader::discover;
use plugin_loader::discover::LoadedPackage;

const SR: f32 = 48_000.0;

/// Re-audited `output_gain_db` in the bundled `marshall_plexi/manifest.yaml`,
/// kept in sync with its OpenRig-plugins source (tone3000 #2717). Single
/// source of truth — the fixture must carry the relative correction the
/// audit emits, not a hot absolute target.
const PLEXI_CAL_DB: f32 = -13.559_131_6;

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

#[test]
fn nam_calibrated_output_does_not_clip_a_normal_level_di() {
    nam::register_builder();

    // Real bundled NAM amp with the audit's real re-audited value (the
    // exact #491 path: manifest output_gain_db → post-model trim).
    let plexi = load("nam/marshall_plexi");
    assert_eq!(
        plexi.manifest.output_gain_db,
        Some(PLEXI_CAL_DB),
        "fixture must mirror the OpenRig-plugins re-audited relative \
         correction, not a hot absolute target (issue #496)"
    );

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
        "NAM clips a normal-level DI: output_gain_db pushed the peak to \
         {peak:.4} (> 1.0 full-scale). A hot absolute calibration was \
         re-introduced — the manifest must carry the audit's bounded \
         relative correction (issue #496)."
    );
}
