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

#[test]
#[ignore]
fn dumble_clean_vs_bogner_synergy_actual_output() {
    let input = pink_noise_peak_normalized(PROBE_SAMPLES, -12.0, 0xC0FFEE);

    let mut dumble = build_dumble_clean();
    let mut bogner = build_bogner_synergy();

    let dumble_out = run(&mut dumble, &input);
    let bogner_out = run(&mut bogner, &input);

    let dumble_pk = peak_dbfs(&dumble_out);
    let dumble_rms = rms_dbfs(&dumble_out);
    let bogner_pk = peak_dbfs(&bogner_out);
    let bogner_rms = rms_dbfs(&bogner_out);

    eprintln!(
        "Dumble Steel SS Clean → peak={dumble_pk:+.2} dBFS  rms={dumble_rms:+.2} dBFS"
    );
    eprintln!(
        "Bogner Synergy Blue 8 → peak={bogner_pk:+.2} dBFS  rms={bogner_rms:+.2} dBFS"
    );
    eprintln!("Δ peak = {:+.2} dB   Δ rms = {:+.2} dB", dumble_pk - bogner_pk, dumble_rms - bogner_rms);

    let diff_rms = (dumble_rms - bogner_rms).abs();
    assert!(
        diff_rms < 3.0,
        "Dumble and Bogner should be within 3 dB RMS, got Δ={diff_rms:.2} dB"
    );
}
