//! No "xiado" (amplified noise floor) on the decay/silence. Issue #496.
//!
//! The user reported a hiss as a note decays: a large post-model gain
//! amplifies the model's noise floor, so when the signal dies the
//! amplified hiss stays audible. This drives the worst hot calibration
//! (+18.68 dB, CPM 22 class) through the real model, then feeds
//! silence, and asserts the silent tail is genuinely quiet — the decay
//! must fall to inaudible, not sit on an amplified hiss plateau.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use domain::value_objects::ParameterValue;
use plugin_loader::discover;
use plugin_loader::discover::LoadedPackage;

const SR: f32 = 48_000.0;
const HOT_CAL_DB: f32 = 18.68;

fn load(rel_dir: &str) -> LoadedPackage {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins");
    let target = root.join(rel_dir);
    discover::discover(&root)
        .expect("discover")
        .into_iter()
        .filter_map(Result::ok)
        .find(|p| p.root == target)
        .expect("fixture")
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

#[test]
fn nam_decay_into_silence_has_no_amplified_hiss() {
    nam::register_builder();

    let mut pkg = load("nam/marshall_plexi");
    pkg.manifest.output_gain_db = Some(HOT_CAL_DB);
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String("angus".into()));
    let mut amp = pkg
        .build_processor(&params, SR, AudioChannelLayout::Mono)
        .expect("build");

    // 0.1 s of DI, then 1.0 s of pure silence — "you stopped playing".
    let di_n = (SR * 0.1) as usize;
    let sil_n = (SR * 1.0) as usize;
    let mut buf: Vec<f32> = (0..di_n)
        .map(|n| 0.3 * (2.0 * std::f32::consts::PI * 110.0 * n as f32 / SR).sin())
        .chain(std::iter::repeat_n(0.0, sil_n))
        .collect();

    match &mut amp {
        BlockProcessor::Mono(m) => m.process_block(&mut buf),
        BlockProcessor::Stereo(_) => panic!("expected mono"),
    }

    assert!(buf.iter().all(|s| s.is_finite()), "NaN/Inf");

    // Last 0.1 s = well into silence. Must have collapsed to inaudible.
    let tail = &buf[buf.len() - (SR * 0.1) as usize..];
    let rms = (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt();
    let rms_db = db(rms);
    assert!(
        rms_db < -60.0,
        "XIADO: silent tail sits at {rms_db:.2} dBFS (≥ -60) — the hot \
         calibration is amplifying the model noise floor into an audible \
         hiss on the decay (issue #496)"
    );
}
