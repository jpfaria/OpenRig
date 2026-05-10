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

fn build_bogner_ecstasy_drive_red() -> BlockProcessor {
    let pkg = load_pkg("bogner_ecstasy");
    let mut params = ParameterSet::default();
    // bogner_ecstasy axes: channel + cabinet
    params.insert("channel", ParameterValue::String("drive_red".into()));
    params.insert("cabinet", ParameterValue::String("4x12_v30".into()));
    nam::from_package::build_from_package(&pkg, &params, 48_000.0, AudioChannelLayout::Mono)
        .expect("build Bogner Ecstasy drive_red")
}

/// Helper that processes the probe input through every supplied
/// (label, processor) pair and asserts the RMS spread stays within
/// `tolerance_db`. Designed to FAIL noisily — every label and number
/// is dumped so the failure tells you exactly which capture broke.
fn assert_aligned(mut entries: Vec<(&'static str, BlockProcessor)>, tolerance_db: f32) {
    let input = pink_noise_peak_normalized(PROBE_SAMPLES, -12.0, 0xC0FFEE);
    let mut measurements = Vec::new();
    for (name, mut p) in entries.drain(..) {
        let out = run(&mut p, &input);
        let pk = peak_dbfs(&out);
        let rms = rms_dbfs(&out);
        eprintln!("{name:32} peak={pk:+.2} dBFS  rms={rms:+.2} dBFS");
        measurements.push((name, rms));
    }
    let max = measurements.iter().map(|(_, r)| *r).fold(f32::MIN, f32::max);
    let min = measurements.iter().map(|(_, r)| *r).fold(f32::MAX, f32::min);
    let spread = max - min;
    eprintln!("RMS spread across {} captures: {:+.2} dB", measurements.len(), spread);
    assert!(
        spread <= tolerance_db,
        "RMS spread {spread:.2} dB exceeds tolerance {tolerance_db:.2} dB. \
         If the probe path is active, every NAM amp/preamp lands within ~1 dB.\n\
         A large spread usually means: (1) the caller is on an old build that doesn't \
         run the probe, or (2) `loudness_normalize` is somehow false for these models."
    );
}

#[test]
#[ignore]
fn dumble_vs_bogner_lineup_must_align() {
    assert_aligned(
        vec![
            ("Dumble Steel SS Clean", build_dumble_clean()),
            ("Bogner Synergy Blue 8", build_bogner_synergy()),
            ("Bogner Ecstasy drive_red v30", build_bogner_ecstasy_drive_red()),
            ("Two-Rock Studio Signature", build_two_rock()),
        ],
        2.0,
    );
}
