//! Objective NAM loudness/clip measurement. Issue #496.
//!
//! Not a pass/fail gate yet — a measurement harness so decisions are
//! driven by numbers, not by ear. Feeds one fixed realistic DI through
//! the SAME real bundled NAM model under three calibration values and
//! prints loudness (RMS dBFS) + true peak (dBFS) for each:
//!
//!   1. raw model (no output_gain_db)        — the "tudo baixo" baseline
//!   2. #491 original hot value (+8.93 dB)   — loud, but clips?
//!   3. current re-audited value (-13.56 dB) — what is heard now
//!
//! Run: `cargo test -p engine --test nam_loudness_measure -- --nocapture`

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

/// Realistic guitar DI: peak ≈ 0.3 (≈ -10 dBFS), normal playing level.
fn di_sine(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| 0.3 * (2.0 * std::f32::consts::PI * 110.0 * n as f32 / SR).sin())
        .collect()
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// Steady-state tail metrics: (rms dBFS, true-peak dBFS).
fn metrics(s: &[f32]) -> (f32, f32) {
    let tail = &s[s.len() / 2..];
    let peak = tail.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    let rms = (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt();
    (db(rms), db(peak))
}

fn measure(label: &str, gain_db: Option<f32>) {
    let mut pkg = load("nam/marshall_plexi");
    pkg.manifest.output_gain_db = gain_db;
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String("angus".into()));
    let mut amp = pkg
        .build_processor(&params, SR, AudioChannelLayout::Mono)
        .expect("build NAM");

    // 0.1 s DI then 1.0 s silence — "you stopped playing".
    let di_n = (SR * 0.1) as usize;
    let sil_n = (SR * 1.0) as usize;
    let mut buf: Vec<f32> = di_sine(di_n)
        .into_iter()
        .chain(std::iter::repeat_n(0.0, sil_n))
        .collect();
    match &mut amp {
        BlockProcessor::Mono(m) => m.process_block(&mut buf),
        BlockProcessor::Stereo(_) => panic!("mono"),
    }
    let finite = buf.iter().all(|s| s.is_finite());
    let (sig_rms, sig_peak) = metrics(&buf[..di_n]);
    // last 0.2 s = settled silence (the xiado region)
    let tail = &buf[buf.len() - (SR * 0.2) as usize..];
    let noise_rms = db((tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt());
    let clips = sig_peak > 0.0;
    let xiado = noise_rms >= -60.0;
    eprintln!(
        "{label:<34} sig_rms={sig_rms:>7.2}  peak={sig_peak:>7.2}  decay_noise={noise_rms:>7.2} dBFS  {}{}{}",
        if clips { "CLIP " } else { "ok " },
        if xiado { "XIADO " } else { "quiet " },
        if finite { "" } else { "NaN!" }
    );
}

#[test]
fn measure_nam_loudness_and_clip() {
    nam::register_builder();
    eprintln!("\n=== NAM marshall_plexi @ DI peak 0.3 (≈ -10 dBFS) ===");
    measure("1. raw model (no calibration)", None);
    measure("2. live target (+8.93 dB)", Some(8.931_892));
    measure("3. hot worst case (+18.68 dB)", Some(18.68));
    eprintln!("=== peak > 0 dBFS = digital clip; rms = loudness ===\n");
}
