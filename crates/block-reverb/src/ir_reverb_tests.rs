use std::fs;
use std::path::{Path, PathBuf};

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor, StereoProcessor};
use domain::value_objects::ParameterValue;
use plugin_loader::manifest::PluginManifest;
use plugin_loader::LoadedPackage;

use super::{build_ir_reverb_from_package, DryWetStereo, PreDelayStereo};

/// A wet stand-in that ignores its input and emits a fixed frame, so a test
/// can prove the dry/wet blend without needing a real IR convolution.
struct ConstWet(f32);

impl StereoProcessor for ConstWet {
    fn process_frame(&mut self, _input: [f32; 2]) -> [f32; 2] {
        [self.0, self.0]
    }
}

/// Identity wet: returns its input untouched, so mix=1 with unit gain and no
/// pre-delay should reproduce the input exactly.
struct IdentityWet;

impl StereoProcessor for IdentityWet {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        input
    }
}

#[test]
fn mix_zero_passes_dry_only() {
    let mut rev = DryWetStereo::new(Box::new(ConstWet(9.0)), 0.0, 1.0, PreDelayStereo::new(0));
    let out = rev.process_frame([0.3, -0.4]);
    assert!(
        (out[0] - 0.3).abs() < 1e-6 && (out[1] - (-0.4)).abs() < 1e-6,
        "mix=0 must be dry-only, got {out:?}"
    );
}

#[test]
fn mix_one_passes_wet_only() {
    let mut rev = DryWetStereo::new(Box::new(IdentityWet), 1.0, 1.0, PreDelayStereo::new(0));
    let out = rev.process_frame([0.25, 0.5]);
    assert!(
        (out[0] - 0.25).abs() < 1e-6 && (out[1] - 0.5).abs() < 1e-6,
        "mix=1 with identity wet must reproduce input, got {out:?}"
    );
}

#[test]
fn mix_one_suppresses_dry() {
    // Wet emits a constant; at mix=1 the dry input must not leak through.
    let mut rev = DryWetStereo::new(Box::new(ConstWet(0.0)), 1.0, 1.0, PreDelayStereo::new(0));
    let out = rev.process_frame([0.8, 0.8]);
    assert!(
        out[0].abs() < 1e-6 && out[1].abs() < 1e-6,
        "mix=1 with silent wet must suppress dry, got {out:?}"
    );
}

#[test]
fn stereo_width_preserved_through_dry_path() {
    // Distinct L/R must stay distinct (no auto-pan / no mono collapse).
    let mut rev = DryWetStereo::new(Box::new(ConstWet(0.0)), 0.3, 1.0, PreDelayStereo::new(0));
    let out = rev.process_frame([0.6, -0.2]);
    assert!(out[0] != out[1], "L/R collapsed: {out:?}");
    assert!(
        (out[0] - 0.7 * 0.6).abs() < 1e-6,
        "left dry scale wrong: {out:?}"
    );
    assert!(
        (out[1] - 0.7 * -0.2).abs() < 1e-6,
        "right dry scale wrong: {out:?}"
    );
}

#[test]
fn wet_gain_scales_wet_path() {
    let mut rev = DryWetStereo::new(Box::new(ConstWet(0.5)), 1.0, 0.5, PreDelayStereo::new(0));
    let out = rev.process_frame([0.0, 0.0]);
    assert!(
        (out[0] - 0.25).abs() < 1e-6 && (out[1] - 0.25).abs() < 1e-6,
        "wet gain 0.5 on wet 0.5 must yield 0.25, got {out:?}"
    );
}

// --- package-loading path (build_ir_reverb_from_package) ----------------

const SR: f32 = 48_000.0;

/// Write an impulse IR WAV (`channels`-wide, impulse of `gain` at sample 0,
/// then `len-1` zeros) and return its path.
fn write_impulse_ir(name: &str, channels: u16, gain: f32, len: usize) -> PathBuf {
    let path = std::env::temp_dir().join(name);
    let spec = hound::WavSpec {
        channels,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(&path, spec).expect("create IR wav");
    for frame in 0..len {
        let sample = if frame == 0 { gain } else { 0.0 };
        for _ in 0..channels {
            writer.write_sample(sample).expect("write sample");
        }
    }
    writer.finalize().expect("finalize IR wav");
    path
}

/// Build a `type: reverb` + `backend: ir` package whose single capture points
/// at `wav_path` (whose parent becomes the package root).
fn reverb_ir_package(wav_path: &Path) -> LoadedPackage {
    let file_name = wav_path.file_name().unwrap().to_str().unwrap();
    let yaml = format!(
        "manifest_version: 1\n\
         id: test_reverb_ir\n\
         display_name: Test Reverb IR\n\
         type: reverb\n\
         backend: ir\n\
         captures:\n\
         \x20 - file: {file_name}\n"
    );
    let manifest: PluginManifest = serde_yaml::from_str(&yaml).expect("parse reverb-ir manifest");
    LoadedPackage {
        root: wav_path.parent().unwrap().to_path_buf(),
        manifest,
    }
}

fn params(pairs: &[(&str, f32)]) -> ParameterSet {
    let mut set = ParameterSet::default();
    for (k, v) in pairs {
        set.insert(*k, ParameterValue::Float(*v));
    }
    set
}

#[test]
fn reverb_ir_manifest_with_type_reverb_and_backend_ir_loads() {
    // Guards the loader question: type: reverb + backend: ir must build.
    let wav = write_impulse_ir("openrig_rev_load.wav", 2, 1.0, 128);
    let pkg = reverb_ir_package(&wav);
    let p = params(&[("mix", 30.0)]);
    let built = build_ir_reverb_from_package(&pkg, &p, SR, AudioChannelLayout::Stereo);
    fs::remove_file(&wav).ok();
    assert!(built.is_ok(), "reverb-ir build failed: {:?}", built.err());
    assert!(matches!(built.unwrap(), BlockProcessor::Stereo(_)));
}

#[test]
fn stereo_reverb_ir_convolves_true_stereo_not_downmixed() {
    // Feed L=impulse, R=silent. A true-stereo convolution keeps the wet
    // energy on L only; a mono-collapse would mirror it onto R.
    let wav = write_impulse_ir("openrig_rev_stereo.wav", 2, 1.0, 128);
    let pkg = reverb_ir_package(&wav);
    let p = params(&[("mix", 100.0)]);
    let BlockProcessor::Stereo(mut proc) =
        build_ir_reverb_from_package(&pkg, &p, SR, AudioChannelLayout::Stereo).unwrap()
    else {
        panic!("expected stereo processor");
    };
    fs::remove_file(&wav).ok();

    let mut energy_l = 0.0f32;
    let mut energy_r = 0.0f32;
    for frame in 0..256 {
        let input = if frame == 0 { [1.0, 0.0] } else { [0.0, 0.0] };
        let out = proc.process_frame(input);
        energy_l += out[0].abs();
        energy_r += out[1].abs();
    }
    assert!(energy_l > 0.5, "left wet tail missing: {energy_l}");
    assert!(
        energy_r < 1e-3,
        "right channel leaked (downmix): {energy_r}"
    );
}

#[test]
fn mix_zero_passes_dry_through_package_path() {
    let wav = write_impulse_ir("openrig_rev_dry.wav", 2, 1.0, 128);
    let pkg = reverb_ir_package(&wav);
    let p = params(&[("mix", 0.0)]);
    let BlockProcessor::Stereo(mut proc) =
        build_ir_reverb_from_package(&pkg, &p, SR, AudioChannelLayout::Stereo).unwrap()
    else {
        panic!("expected stereo processor");
    };
    fs::remove_file(&wav).ok();
    let out = proc.process_frame([0.6, -0.3]);
    assert!(
        (out[0] - 0.6).abs() < 1e-6 && (out[1] - (-0.3)).abs() < 1e-6,
        "mix=0 must pass dry, got {out:?}"
    );
}

#[test]
fn mono_ir_in_mono_layout_produces_wet_tail() {
    let wav = write_impulse_ir("openrig_rev_mono.wav", 1, 1.0, 128);
    let pkg = reverb_ir_package(&wav);
    let p = params(&[("mix", 100.0)]);
    let BlockProcessor::Mono(mut proc) =
        build_ir_reverb_from_package(&pkg, &p, SR, AudioChannelLayout::Mono).unwrap()
    else {
        panic!("expected mono processor");
    };
    fs::remove_file(&wav).ok();
    let mut energy = 0.0f32;
    for frame in 0..256 {
        let input = if frame == 0 { 1.0 } else { 0.0 };
        energy += proc.process_sample(input).abs();
    }
    assert!(energy > 0.5, "mono wet tail missing: {energy}");
}

#[test]
fn pre_delay_shifts_wet_in_time() {
    // Identity wet + 2-sample pre-delay: an impulse on the wet path must
    // appear 2 frames later. mix=1 isolates the wet path.
    let mut rev = DryWetStereo::new(Box::new(IdentityWet), 1.0, 1.0, PreDelayStereo::new(2));
    let f0 = rev.process_frame([1.0, 1.0]);
    let f1 = rev.process_frame([0.0, 0.0]);
    let f2 = rev.process_frame([0.0, 0.0]);
    assert!(
        f0[0].abs() < 1e-6,
        "frame 0 should be delayed silence, got {f0:?}"
    );
    assert!(
        f1[0].abs() < 1e-6,
        "frame 1 should be delayed silence, got {f1:?}"
    );
    assert!(
        (f2[0] - 1.0).abs() < 1e-6,
        "impulse should land at frame 2, got {f2:?}"
    );
}
