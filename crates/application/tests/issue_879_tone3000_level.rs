//! #879: an installed TONE3000 package ships the same `output_gain_db` the
//! curated catalog gets from OpenRig-plugins' `loudness_audit`: the reference
//! output peaks at -1 dBFS, clamped to the Output knob range.

use std::path::{Path, PathBuf};

use application::tone3000::convolve::convolve;
use application::tone3000::ir_wav::load_ir_mono_48k;
use application::tone3000::level_nam::{nam_level_peaks_dbfs, nam_output_gain_db};
use application::tone3000::level_policy::{
    amp_level_probes, ir_level_peak_dbfs, peak_dbfs, target_gain_db, IrRole, OUTPUT_KNOB_MAX_DB,
    OUTPUT_KNOB_MIN_DB, TARGET_PEAK_DBFS,
};
use application::tone3000::synthetic_di::{default_guitar_di, DI_SAMPLE_RATE};

fn crest_db(x: &[f32]) -> f32 {
    let rms = (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
    peak_dbfs(x) - 20.0 * rms.log10()
}

fn ts9_captures() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/plugins/nam/ts9_grid/captures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "nam"))
        .collect();
    files.sort();
    files
}

fn write_wav(path: &Path, channels: u16, rate: u32, frames: &[i16]) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for sample in frames {
        writer.write_sample(*sample).unwrap();
    }
    writer.finalize().unwrap();
}

#[test]
fn the_di_is_twelve_seconds_at_48k() {
    assert_eq!(DI_SAMPLE_RATE, 48_000.0);
    assert_eq!(default_guitar_di().len(), 12 * 48_000);
}

#[test]
fn the_di_peaks_at_minus_15_dbfs() {
    let peak = peak_dbfs(&default_guitar_di());
    assert!((peak - -15.0).abs() < 0.01, "peak was {peak:.4} dBFS");
}

#[test]
fn the_di_is_deterministic() {
    assert_eq!(default_guitar_di(), default_guitar_di());
}

#[test]
fn the_di_has_audio_in_every_chord() {
    let di = default_guitar_di();
    for center_s in [1.0_f32, 5.0, 9.0] {
        let start = (center_s * 48_000.0) as usize;
        let window = &di[start..start + 24_000];
        let rms = (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt();
        assert!(rms > 0.01, "window at {center_s}s has RMS {rms:.4}");
    }
}

#[test]
fn silence_reads_minus_120_dbfs() {
    assert_eq!(peak_dbfs(&[0.0; 16]), -120.0);
}

#[test]
fn a_quiet_block_is_boosted_and_a_hot_one_cut() {
    assert!((target_gain_db(-13.0) - 12.0).abs() < 1e-4);
    assert!((target_gain_db(5.0) - -6.0).abs() < 1e-4);
}

#[test]
fn the_gain_stays_inside_the_output_knob() {
    assert_eq!(TARGET_PEAK_DBFS, -1.0);
    assert_eq!(target_gain_db(-60.0), OUTPUT_KNOB_MAX_DB);
    assert_eq!(target_gain_db(40.0), OUTPUT_KNOB_MIN_DB);
    assert_eq!((OUTPUT_KNOB_MIN_DB, OUTPUT_KNOB_MAX_DB), (-24.0, 24.0));
}

#[test]
fn both_cab_probes_peak_at_the_target() {
    for probe in amp_level_probes(&default_guitar_di()) {
        assert!((peak_dbfs(&probe) - TARGET_PEAK_DBFS).abs() < 0.01);
    }
}

#[test]
fn the_saturated_probe_is_denser_than_the_clean_one() {
    let [clean, driven] = amp_level_probes(&default_guitar_di());
    assert!(crest_db(&driven) + 6.0 < crest_db(&clean));
}

#[test]
fn a_unit_ir_leaves_the_cab_at_target_and_the_body_at_the_di_peak() {
    let di = default_guitar_di();
    let delta = [1.0_f32];
    assert!((ir_level_peak_dbfs(&delta, &di, IrRole::Cab) - TARGET_PEAK_DBFS).abs() < 0.01);
    assert!((ir_level_peak_dbfs(&delta, &di, IrRole::Body) - peak_dbfs(&di)).abs() < 0.01);
}

#[test]
fn the_cab_level_follows_the_ir_gain() {
    let peak = ir_level_peak_dbfs(&[2.0_f32], &default_guitar_di(), IrRole::Cab);
    assert!((peak - (TARGET_PEAK_DBFS + 6.0206)).abs() < 0.01);
}

#[test]
fn a_delta_ir_convolves_to_the_signal() {
    let sig = [0.1, -0.4, 0.7, 0.2, -0.9];
    let y = convolve(&sig, &[0.5]);
    assert_eq!(y.len(), sig.len());
    for (a, b) in sig.iter().zip(&y) {
        assert!((a * 0.5 - b).abs() < 1e-5);
    }
    assert!(convolve(&[], &[1.0]).is_empty());
}

#[test]
fn convolution_matches_the_naive_sum() {
    let sig: Vec<f32> = (0..200).map(|i| (i as f32 * 0.3).sin()).collect();
    let ir: Vec<f32> = (0..37).map(|i| (i as f32 * 0.11).cos() * 0.2).collect();
    let fast = convolve(&sig, &ir);
    let mut naive = vec![0.0_f32; sig.len() + ir.len() - 1];
    for (i, s) in sig.iter().enumerate() {
        for (j, h) in ir.iter().enumerate() {
            naive[i + j] += s * h;
        }
    }
    assert_eq!(fast.len(), naive.len());
    for (a, b) in fast.iter().zip(&naive) {
        assert!((a - b).abs() < 1e-3, "{a} vs {b}");
    }
}

#[test]
fn a_mono_48k_wav_loads_as_is() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mono.wav");
    write_wav(&path, 1, 48_000, &[0, 16384, -16384, 32767]);
    let ir = load_ir_mono_48k(&path).unwrap();
    assert_eq!(ir.len(), 4);
    assert!((ir[1] - 0.5).abs() < 1e-3);
    assert!((ir[3] - 1.0).abs() < 1e-3);
}

#[test]
fn a_stereo_44k_wav_folds_to_mono_at_48k() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stereo.wav");
    let frames: Vec<i16> = (0..441).flat_map(|_| [16384_i16, 0]).collect();
    write_wav(&path, 2, 44_100, &frames);
    let ir = load_ir_mono_48k(&path).unwrap();
    assert!((ir.len() as i32 - 480).abs() <= 1, "len was {}", ir.len());
    let middle = ir[ir.len() / 2];
    assert!((middle - 0.25).abs() < 1e-2, "mono mean was {middle}");
}

#[test]
fn a_missing_wav_is_an_error() {
    assert!(load_ir_mono_48k(Path::new("/nonexistent/ir.wav")).is_err());
}

#[test]
fn every_nam_capture_is_measured() {
    let files = ts9_captures();
    assert_eq!(files.len(), 2);
    let peaks = nam_level_peaks_dbfs(&default_guitar_di(), &files).unwrap();
    assert_eq!(peaks.len(), 2);
    for peak in &peaks {
        assert!(peak.is_finite() && *peak > -120.0, "peak {peak}");
    }
}

#[test]
fn the_nam_gain_keeps_the_loudest_capture_at_the_target() {
    let files = ts9_captures();
    let di = default_guitar_di();
    let peaks = nam_level_peaks_dbfs(&di, &files).unwrap();
    let loudest = peaks.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let gain = nam_output_gain_db(&di, &files).unwrap();
    assert!((gain - target_gain_db(loudest)).abs() < 1e-4);
}

#[test]
fn a_broken_nam_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.nam");
    std::fs::write(&path, b"not a model").unwrap();
    assert!(nam_output_gain_db(&default_guitar_di(), &[path]).is_err());
}
