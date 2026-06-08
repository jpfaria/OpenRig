//! Issue #657: NAM A2 Slimmable size must be a user-adjustable lever.
//!
//! A2 `.nam` captures are `SlimmableContainer`s: an array of WaveNet
//! submodels at increasing channel counts (0.5 Lite / 1.0 Full). The
//! upstream lib exposes `nam::SlimmableModel::SetSlimmableSize(0.0..1.0)`
//! to pick the active submodel at runtime, trading fidelity for CPU.
//!
//! Before this issue OpenRig never called it: `cpp/nam_wrapper::nam_create`
//! built the DSP via `nam::get_dsp` and never `dynamic_cast`ed to
//! `SlimmableModel`, so every A2 model silently ran at full size and the
//! `slim` knob did not exist. This test proves the knob is now wired all
//! the way through the FFI: the same model + same input produces a
//! DIFFERENT output at `slim = 0.0` (smallest submodel) than at the
//! default `slim = 1.0` (full). If the lever is ignored the two outputs
//! are bit-identical and this fails.
//!
//! Same harness as `issue_623_nam_a2_slimmable.rs`: real processor from
//! the bundled A2 fixture, real sine through `process_block`.

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

fn build_a2(pkg: &LoadedPackage, slim: Option<f32>) -> BlockProcessor {
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String("a2".into()));
    if let Some(s) = slim {
        params.insert("slim", ParameterValue::Float(s));
    }
    pkg.build_processor(&params, SR, AudioChannelLayout::Mono)
        .expect("build A2 NAM")
}

#[test]
fn nam_a2_slim_param_changes_output_vs_full() {
    nam::register_builder();
    let pkg = load("nam/a2_slimmable");

    let input = di_sine(8_192);

    // Default = full size (1.0): the historical, unaffected behavior.
    let out_full = run_mono(&mut build_a2(&pkg, None), &input);
    // Slim = 0.0: smallest submodel via SetSlimmableSize.
    let out_slim = run_mono(&mut build_a2(&pkg, Some(0.0)), &input);

    assert!(
        out_slim.iter().all(|s| s.is_finite()),
        "slim output produced NaN/Inf"
    );

    let max_diff = out_full
        .iter()
        .zip(&out_slim)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);

    assert!(
        max_diff > 1e-3,
        "slim=0.0 and slim=1.0 produced (near-)identical output \
         (max_diff={max_diff:e}) — SetSlimmableSize is not wired through \
         the FFI; the slim knob is being ignored (issue #657)"
    );
}
