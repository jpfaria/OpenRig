//! Issue #623: NAM A2 / SlimmableContainer must process without crashing.
//!
//! The A2 `.nam` capture has a top-level `architecture: SlimmableContainer`
//! (version 0.7.0) whose `config.submodels` is an array of WaveNet
//! submodels at increasing `max_value` (0.5 Lite / 1.0 Full). With the
//! NeuralAmpModelerCore submodule pinned before the upstream slimmable
//! runtime fixes (#258/#259/#260/#267), the model LOADS but SIGSEGVs the
//! moment a buffer is pushed through inference (the slimmable container
//! prewarming bug, upstream #259).
//!
//! Same harness as `nam_output_gain_no_clip.rs`: build the real processor
//! from the bundled fixture and run a real sine through `process_block`.
//! The gate is simply: it must not crash, and the output must be all
//! finite and non-trivial (the inference actually ran).

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

fn run_mono(proc: &mut BlockProcessor, input: &[f32]) -> Vec<f32> {
    let mut buf = input.to_vec();
    match proc {
        BlockProcessor::Mono(m) => m.process_block(&mut buf),
        BlockProcessor::Stereo(_) => panic!("expected mono"),
    }
    buf
}

#[test]
fn nam_a2_slimmable_processes_without_crash_and_is_finite() {
    nam::register_builder();

    let pkg = load("nam/a2_slimmable");
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String("a2".into()));
    let mut amp = pkg
        .build_processor(&params, SR, AudioChannelLayout::Mono)
        .expect("build A2 NAM");

    let out = run_mono(&mut amp, &di_sine(4_096));

    assert!(
        out.iter().all(|s| s.is_finite()),
        "A2 SlimmableContainer produced NaN/Inf (issue #623)"
    );
    let rms = (out.iter().map(|v| v * v).sum::<f32>() / out.len() as f32).sqrt();
    assert!(
        rms > 1e-6,
        "A2 SlimmableContainer produced (near-)silence rms={rms:e} — \
         inference did not run (issue #623)"
    );
}
