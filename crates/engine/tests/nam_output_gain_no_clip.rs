//! NAM must be LOUD and NOT clip, even with a hot calibration. Issue #496.
//!
//! Objective gate (numbers, not ears). One fixed realistic DI through
//! the real bundled NAM model carrying the worst documented hot value
//! (#496: the CPM 22 class shipped +18.68 dB). The output must satisfy
//! ALL THREE simultaneously:
//!
//!   * finite              — no NaN/Inf
//!   * peak ≤ 0 dBFS       — no digital clip / harsh distortion
//!   * rms  ≥ -18 dBFS     — still loud (a quiet "fix" is not a fix)
//!
//! Before #496 the hot value pushed peak to +8.7 dBFS (clip). A blind
//! ceiling killed loudness instead. The fix is a memoryless soft-clip
//! on the NAM output: transparent for normal signal, smoothly bounded
//! at the peaks that would clip — loud AND clean, zero latency.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use domain::value_objects::ParameterValue;
use plugin_loader::discover;
use plugin_loader::discover::LoadedPackage;

const SR: f32 = 48_000.0;

/// Worst real hot `output_gain_db` documented in issue #496 (CPM 22
/// class). The gate must hold even here.
const HOT_CAL_DB: f32 = 18.68;

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

/// Realistic guitar DI: peak ≈ 0.3 (≈ -10 dBFS), normal playing level.
fn di_sine(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| 0.3 * (2.0 * std::f32::consts::PI * 110.0 * n as f32 / SR).sin())
        .collect()
}

fn run_mono(proc: &mut BlockProcessor, input: &[f32]) -> Vec<f32> {
    let mut buf = input.to_vec();
    match proc {
        BlockProcessor::Mono(m) => m.process_block(&mut buf),
        BlockProcessor::Stereo(_) => panic!("expected mono"),
    }
    buf
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

#[test]
fn nam_is_loud_and_does_not_clip_even_with_a_hot_calibration() {
    nam::register_builder();

    let mut pkg = load("nam/marshall_plexi");
    pkg.manifest.output_gain_db = Some(HOT_CAL_DB);
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String("angus".into()));
    let mut amp = pkg
        .build_processor(&params, SR, AudioChannelLayout::Mono)
        .expect("build NAM");

    let out = run_mono(&mut amp, &di_sine(8_192));
    let tail = &out[out.len() / 2..];
    let peak = tail.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    let rms = (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt();
    let (peak_db, rms_db) = (db(peak), db(rms));

    assert!(out.iter().all(|s| s.is_finite()), "produced NaN/Inf");
    assert!(
        peak <= 1.0,
        "CLIPS: peak {peak_db:.2} dBFS (> 0) with a {HOT_CAL_DB:.2} dB \
         calibration — digital clip / harsh distortion (issue #496)"
    );
    assert!(
        rms_db >= -18.0,
        "TOO QUIET: rms {rms_db:.2} dBFS (< -18) — a quiet fix is not a \
         fix; loudness must survive the clip safety (issue #496)"
    );
}
