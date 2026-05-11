//! Integration test: load Dumble Steel-String Singer + Synergy Bogner
//! Ecstasy through the SAME path the running app uses, process the
//! exact same probe signal through both, and report what comes out.
//! No guessing — measure.
//!
//! Reads .nam files from the user's local OpenRig-plugins checkout.
//! Marked `#[ignore]` so CI / `cargo test --workspace` doesn't try to
//! load files that aren't part of this repo. Run with:
//!
//!     cargo test -p nam --test loudness_alignment -- --ignored --nocapture

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use domain::value_objects::ParameterValue;
use plugin_loader::manifest::PluginManifest;
use plugin_loader::LoadedPackage;

const PLUGINS_NAM_ROOT: &str =
    "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source/nam";

const PROBE_SAMPLES: usize = 96_000;

fn load_pkg(plugin_subdir: &str) -> LoadedPackage {
    let root = PathBuf::from(format!("{PLUGINS_NAM_ROOT}/{plugin_subdir}"));
    let yaml = std::fs::read_to_string(root.join("manifest.yaml"))
        .unwrap_or_else(|e| panic!("read manifest for {plugin_subdir}: {e}"));
    let manifest: PluginManifest = serde_yaml::from_str(&yaml)
        .unwrap_or_else(|e| panic!("parse manifest for {plugin_subdir}: {e}"));
    LoadedPackage { root, manifest }
}

fn pink_noise_peak_normalized(samples: usize, peak_dbfs: f32, seed: u64) -> Vec<f32> {
    let mut state = if seed == 0 { 0xDEAD_BEEF } else { seed };
    let mut next = || {
        let mut x = state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state = x;
        ((x as f64 / u64::MAX as f64) as f32) * 2.0 - 1.0
    };
    const OCT: usize = 8;
    let mut rolls = [0.0_f32; OCT];
    for r in rolls.iter_mut() {
        *r = next();
    }
    let mut buf = Vec::with_capacity(samples);
    for n in 0..samples {
        for (i, r) in rolls.iter_mut().enumerate() {
            if (n as u64) & (1u64 << i) == 0 {
                *r = next();
            }
        }
        buf.push(rolls.iter().sum::<f32>() + next());
    }
    let peak = buf.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
    if peak > 0.0 {
        let target = 10.0_f32.powf(peak_dbfs / 20.0);
        let scale = target / peak;
        for s in buf.iter_mut() {
            *s *= scale;
        }
    }
    buf
}

fn run(processor: &mut BlockProcessor, input: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(input.len());
    match processor {
        BlockProcessor::Mono(p) => {
            for &s in input {
                out.push(p.process_sample(s));
            }
        }
        BlockProcessor::Stereo(_) => panic!("expected mono processor for NAM"),
    }
    out
}

fn peak_dbfs(buf: &[f32]) -> f32 {
    let p = buf.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
    if p == 0.0 {
        -120.0
    } else {
        20.0 * p.log10()
    }
}

fn rms_dbfs(buf: &[f32]) -> f32 {
    let m = buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32;
    if m == 0.0 {
        -120.0
    } else {
        10.0 * m.log10()
    }
}

fn build_dumble_clean() -> BlockProcessor {
    let pkg = load_pkg("dumble_steel_string_singer");
    let mut params = ParameterSet::default();
    params.insert("channel", ParameterValue::String("clean".into()));
    params.insert("variant", ParameterValue::String("default".into()));
    nam::from_package::build_from_package(&pkg, &params, 48_000.0, AudioChannelLayout::Mono)
        .expect("build Dumble")
}

fn build_bogner_synergy() -> BlockProcessor {
    let pkg = load_pkg("synergy_bogner_ecstasy");
    let mut params = ParameterSet::default();
    // synergy_bogner_ecstasy has 2 axes — pick a known capture.
    params.insert("channel", ParameterValue::String("blue".into()));
    params.insert("variant", ParameterValue::String("8".into()));
    nam::from_package::build_from_package(&pkg, &params, 48_000.0, AudioChannelLayout::Mono)
        .expect("build Bogner Synergy")
}

fn build_two_rock() -> BlockProcessor {
    let pkg = load_pkg("two_rock_studio_signature");
    // The first capture in the manifest — use it directly.
    let params = ParameterSet::default();
    nam::from_package::build_from_package(&pkg, &params, 48_000.0, AudioChannelLayout::Mono)
        .expect("build Two-Rock")
}

fn build_klon_centaur() -> BlockProcessor {
    let pkg = load_pkg("klon_centaur");
    let params = ParameterSet::default();
    nam::from_package::build_from_package(&pkg, &params, 48_000.0, AudioChannelLayout::Mono)
        .expect("build Klon Centaur")
}

fn build_ts9_default() -> BlockProcessor {
    let pkg = load_pkg("ibanez_ts9");
    // TS9 has axes for drive/tone/level — pick a typical "drive 7" capture.
    let mut params = ParameterSet::default();
    params.insert("drive", ParameterValue::Float(7.0));
    params.insert("tone", ParameterValue::Float(7.0));
    params.insert("level", ParameterValue::Float(7.0));
    nam::from_package::build_from_package(&pkg, &params, 48_000.0, AudioChannelLayout::Mono)
        .expect("build TS9")
}

fn build_proco_rat() -> BlockProcessor {
    let pkg = load_pkg("proco_rat");
    let params = ParameterSet::default();
    nam::from_package::build_from_package(&pkg, &params, 48_000.0, AudioChannelLayout::Mono)
        .expect("build ProCo RAT")
}

/// Run two mono processors back-to-back and return the second's output.
fn chain_two(first: &mut BlockProcessor, second: &mut BlockProcessor, input: &[f32]) -> Vec<f32> {
    let mid = run(first, input);
    run(second, &mid)
}

/// Diagnostic helper for `gain pedal → amp` chains. Same as
/// `dump_outputs`, but feeds the input through the pedal first.
fn dump_chain_outputs(mut entries: Vec<(&'static str, BlockProcessor, BlockProcessor)>) {
    let input = pink_noise_peak_normalized(PROBE_SAMPLES, -12.0, 0xC0FFEE);
    for (name, mut pedal, mut amp) in entries.drain(..) {
        let out = chain_two(&mut pedal, &mut amp, &input);
        let pk = peak_dbfs(&out);
        let rms = rms_dbfs(&out);
        eprintln!("{name:48} peak={pk:+.2}  rms={rms:+.2}");
        assert!(
            out.iter().all(|s| s.is_finite()),
            "{name} produced non-finite samples"
        );
    }
}

fn build_bogner_ecstasy_drive_red() -> BlockProcessor {
    let pkg = load_pkg("bogner_ecstasy");
    let mut params = ParameterSet::default();
    // bogner_ecstasy axes: channel + cabinet
    params.insert("channel", ParameterValue::String("drive_red".into()));
    params.insert("cabinet", ParameterValue::String("4x12_v30".into()));
    nam::from_package::build_from_package(&pkg, &params, 48_000.0, AudioChannelLayout::Mono)
        .expect("build Bogner Ecstasy drive_red")
}

/// Diagnostic helper — runs each NAM through the probe input, dumps
/// peak / RMS for inspection, asserts the output is finite. Does NOT
/// assert alignment: loudness alignment now lives in the manifest's
/// `output_gain_db` field, populated offline by `nam_loudness_audit`
/// and summed onto `output_level_db` at build time. NAMs measured in
/// isolation here deliver their natural level.
fn dump_outputs(mut entries: Vec<(&'static str, BlockProcessor)>) {
    let input = pink_noise_peak_normalized(PROBE_SAMPLES, -12.0, 0xC0FFEE);
    for (name, mut p) in entries.drain(..) {
        let out = run(&mut p, &input);
        let pk = peak_dbfs(&out);
        let rms = rms_dbfs(&out);
        eprintln!("{name:48} peak={pk:+.2}  rms={rms:+.2}");
        assert!(
            out.iter().all(|s| s.is_finite()),
            "{name} produced non-finite samples"
        );
    }
}

#[test]
#[ignore]
fn dumble_vs_bogner_lineup_dumps_levels() {
    dump_outputs(vec![
        ("Dumble Steel SS Clean", build_dumble_clean()),
        ("Bogner Synergy Blue 8", build_bogner_synergy()),
        (
            "Bogner Ecstasy drive_red v30",
            build_bogner_ecstasy_drive_red(),
        ),
        ("Two-Rock Studio Signature", build_two_rock()),
    ]);
}

#[test]
#[ignore]
fn dumble_vs_bogner_with_gain_pedal_in_front_dumps_levels() {
    // Same gain pedal, swap amp downstream — what the user does in
    // the chain (klon → Dumble vs klon → Bogner).
    dump_chain_outputs(vec![
        ("Klon → Dumble", build_klon_centaur(), build_dumble_clean()),
        (
            "Klon → Bogner Synergy",
            build_klon_centaur(),
            build_bogner_synergy(),
        ),
        (
            "Klon → Bogner Ecstasy",
            build_klon_centaur(),
            build_bogner_ecstasy_drive_red(),
        ),
        ("Klon → Two-Rock", build_klon_centaur(), build_two_rock()),
        ("TS9 → Dumble", build_ts9_default(), build_dumble_clean()),
        (
            "TS9 → Bogner Ecstasy",
            build_ts9_default(),
            build_bogner_ecstasy_drive_red(),
        ),
        ("RAT → Dumble", build_proco_rat(), build_dumble_clean()),
        (
            "RAT → Bogner Ecstasy",
            build_proco_rat(),
            build_bogner_ecstasy_drive_red(),
        ),
    ]);
}
