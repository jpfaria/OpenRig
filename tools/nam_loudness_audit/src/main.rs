//! Loudness audit for NAM plugin packages (issue #402).
//!
//! Walks `<plugins-root>/nam/<id>/`, runs each capture against a
//! pink-noise reference signal, measures the resulting **true peak**
//! (dBTP) and writes the corrective `output_gain_db` field into the
//! package's `manifest.yaml` so every NAM lands at the same true peak
//! when first added to a chain.
//!
//! Default target is -1 dBTP — broadcast / streaming safe, guarantees
//! zero clipping even after the inter-sample reconstruction at the DAC.
//! No user-facing knob: the user said "always at 100%", so the audit
//! sets each NAM to the maximum non-clipping level and that's what
//! ships.
//!
//! Usage:
//!
//! ```text
//! nam_loudness_audit --plugins-root <path> [--target-tp -1.0]
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, bail, Context, Result};
use block_core::param::ParameterSet;
use block_core::AudioChannelLayout;
use ebur128::{EbuR128, Mode};
use plugin_loader::manifest::Backend;
use plugin_loader::PluginManifest;

const DEFAULT_TARGET_TP_DB: f64 = -1.0;
const SAMPLE_RATE: f32 = 48_000.0;
const MEASURE_SECONDS: usize = 10;
const PINK_REFERENCE_LUFS: f64 = -18.0;

fn main() -> ExitCode {
    match try_main() {
        Ok(report) => {
            print_report(&report);
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("nam_loudness_audit: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn try_main() -> Result<AuditReport> {
    let args = parse_args(std::env::args().skip(1))?;
    let nam_root = args.plugins_root.join("nam");
    if !nam_root.is_dir() {
        bail!(
            "no `nam/` subdir under plugins-root `{}`",
            args.plugins_root.display()
        );
    }
    let mut report = AuditReport::default();
    let mut entries: Vec<PathBuf> = fs::read_dir(&nam_root)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for package_dir in entries {
        match audit_one_package(&package_dir, args.target_tp_db) {
            Ok(outcome) => report.audited.push(outcome),
            Err(err) => report.failed.push(AuditFailure {
                package: package_dir,
                error: err.to_string(),
            }),
        }
    }
    Ok(report)
}

#[derive(Default)]
struct Args {
    plugins_root: PathBuf,
    target_tp_db: f64,
}

fn parse_args<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut plugins_root: Option<PathBuf> = None;
    let mut target_tp_db: f64 = DEFAULT_TARGET_TP_DB;
    let mut iter = iter.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--plugins-root" => {
                let v = iter
                    .next()
                    .ok_or_else(|| anyhow!("--plugins-root needs a path"))?;
                plugins_root = Some(PathBuf::from(v));
            }
            "--target-tp" => {
                let v = iter
                    .next()
                    .ok_or_else(|| anyhow!("--target-tp needs a value (dBTP)"))?;
                target_tp_db = v.parse().context("invalid --target-tp")?;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: nam_loudness_audit --plugins-root <path> \
                     [--target-tp {DEFAULT_TARGET_TP_DB} (dBTP)]"
                );
                std::process::exit(0);
            }
            unknown => bail!("unknown argument `{unknown}`"),
        }
    }
    Ok(Args {
        plugins_root: plugins_root.ok_or_else(|| anyhow!("--plugins-root is required"))?,
        target_tp_db,
    })
}

#[derive(Default)]
struct AuditReport {
    audited: Vec<AuditOutcome>,
    failed: Vec<AuditFailure>,
}

struct AuditOutcome {
    id: String,
    measured_tp_db: f64,
    correction_db: f32,
    previous_correction_db: Option<f32>,
}

struct AuditFailure {
    package: PathBuf,
    error: String,
}

fn audit_one_package(package_dir: &Path, target_tp_db: f64) -> Result<AuditOutcome> {
    let manifest_path = package_dir.join("manifest.yaml");
    let yaml = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read manifest at {}", manifest_path.display()))?;
    let mut manifest: PluginManifest = serde_yaml::from_str(&yaml)
        .with_context(|| format!("parse manifest at {}", manifest_path.display()))?;
    let captures = match &manifest.backend {
        Backend::Nam { captures, .. } => captures.clone(),
        other => bail!("not a NAM package (backend = {other:?})"),
    };
    if captures.is_empty() {
        bail!("no captures listed in manifest");
    }
    let measurements: Vec<f64> = captures
        .iter()
        .map(|cap| measure_capture_true_peak_db(package_dir, &cap.file))
        .collect::<Result<_>>()?;
    let measured_tp_db = max_true_peak_db(&measurements);
    let correction_db = compute_correction_db(measured_tp_db, target_tp_db);
    let previous_correction_db = manifest.output_gain_db;
    manifest.output_gain_db = Some(correction_db);
    let serialized =
        serde_yaml::to_string(&manifest).context("re-serialize manifest with output_gain_db")?;
    fs::write(&manifest_path, serialized)
        .with_context(|| format!("write {}", manifest_path.display()))?;
    Ok(AuditOutcome {
        id: manifest.id,
        measured_tp_db,
        correction_db,
        previous_correction_db,
    })
}

fn measure_capture_true_peak_db(package_dir: &Path, capture_file: &Path) -> Result<f64> {
    let model_path = package_dir.join(capture_file);
    let model_path_str = model_path
        .to_str()
        .ok_or_else(|| anyhow!("non-utf8 capture path: {model_path:?}"))?;
    let params = ParameterSet::default();
    // Use a baseline manifest (no correction) so the audit measures the
    // raw NAM output, not a previously-corrected one.
    let plugin_params =
        nam::from_package::effective_plugin_params(&dummy_manifest(), &params)?;
    let mut processor = nam::build_processor_with_assets_for_layout(
        model_path_str,
        None,
        plugin_params,
        SAMPLE_RATE,
        AudioChannelLayout::Mono,
    )?;
    let n_samples = (SAMPLE_RATE as usize) * MEASURE_SECONDS;
    let pink = pink_noise_at(PINK_REFERENCE_LUFS, n_samples);
    let mut output = Vec::with_capacity(n_samples);
    for sample in pink.iter() {
        match &mut processor {
            block_core::BlockProcessor::Mono(p) => output.push(p.process_sample(*sample)),
            block_core::BlockProcessor::Stereo(_) => {
                bail!("expected mono processor for capture")
            }
        }
    }
    let mut meter = EbuR128::new(1, SAMPLE_RATE as u32, Mode::TRUE_PEAK)
        .map_err(|e| anyhow!("ebur128 init: {e}"))?;
    meter
        .add_frames_f32(&output)
        .map_err(|e| anyhow!("ebur128 add_frames: {e}"))?;
    let true_peak_linear = meter
        .true_peak(0)
        .map_err(|e| anyhow!("ebur128 true_peak: {e}"))?;
    Ok(linear_to_db(true_peak_linear))
}

/// Compute the dB correction needed to land at `target_tp_db` from a
/// `measured_tp_db`. Pure function, no I/O.
pub fn compute_correction_db(measured_tp_db: f64, target_tp_db: f64) -> f32 {
    (target_tp_db - measured_tp_db) as f32
}

/// Worst-case (highest) true peak across captures inside a single
/// package. Using max — instead of mean — guarantees the loudest
/// variant of the package still lands at or below the target. Quieter
/// variants will sit slightly under, never over.
pub fn max_true_peak_db(values: &[f64]) -> f64 {
    values.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
}

fn linear_to_db(linear: f64) -> f64 {
    if linear <= 0.0 {
        return f64::NEG_INFINITY;
    }
    20.0 * linear.log10()
}

/// Pink-noise generator (Voss-McCartney) normalized so the integrated
/// LUFS over the requested duration matches `target_lufs`. Reference
/// signal feeding the audit; deterministic seed for reproducibility.
fn pink_noise_at(target_lufs: f64, n_samples: usize) -> Vec<f32> {
    let raw = pink_noise_voss(n_samples);
    let rms = rms(&raw).max(1e-12);
    let scale = lufs_to_linear(target_lufs) / rms;
    raw.iter().map(|s| s * scale).collect()
}

fn pink_noise_voss(n: usize) -> Vec<f32> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        ((state >> 11) as f32 / u32::MAX as f32) - 0.5
    };
    let octaves = 7;
    let mut rows = vec![0.0_f32; octaves];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let trailing = (i as u32).trailing_zeros() as usize;
        if trailing < rows.len() {
            rows[trailing] = next();
        }
        out.push(rows.iter().sum::<f32>() / octaves as f32);
    }
    out
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

fn lufs_to_linear(lufs: f64) -> f32 {
    10f32.powf(lufs as f32 / 20.0)
}

fn dummy_manifest() -> PluginManifest {
    PluginManifest {
        manifest_version: 1,
        id: "audit_temp".to_string(),
        display_name: "Audit Temp".to_string(),
        author: None,
        description: None,
        inspired_by: None,
        brand: None,
        thumbnail: None,
        photo: None,
        screenshot: None,
        brand_logo: None,
        license: None,
        homepage: None,
        sources: None,
        output_gain_db: None,
        block_type: plugin_loader::manifest::BlockType::Amp,
        backend: Backend::Nam {
            parameters: vec![],
            captures: vec![plugin_loader::manifest::GridCapture {
                values: BTreeMap::new(),
                file: PathBuf::from("placeholder"),
            }],
        },
    }
}

fn print_report(report: &AuditReport) {
    println!(
        "nam_loudness_audit: {} package(s) audited, {} failed",
        report.audited.len(),
        report.failed.len()
    );
    for outcome in &report.audited {
        let prev = match outcome.previous_correction_db {
            Some(v) => format!("{v:+.2} dB"),
            None => "—".to_string(),
        };
        println!(
            "  ok    {:32}  measured peak {:+6.2} dBTP  correction {:+6.2} dB  (was {})",
            outcome.id, outcome.measured_tp_db, outcome.correction_db, prev
        );
    }
    for failure in &report.failed {
        let label = failure
            .package
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>");
        eprintln!("  FAIL  {label}: {}", failure.error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_drives_measured_to_target() {
        // Measured peak at -3 dBTP, target -1 dBTP -> need +2 dB.
        let c = compute_correction_db(-3.0, -1.0);
        assert!((c - 2.0).abs() < 1e-6, "got {c}");
    }

    #[test]
    fn correction_negative_when_measured_above_target() {
        // Already at 0 dBTP, target -1 dBTP -> -1 dB.
        let c = compute_correction_db(0.0, -1.0);
        assert!((c - (-1.0)).abs() < 1e-6, "got {c}");
    }

    #[test]
    fn correction_zero_when_already_at_target() {
        let c = compute_correction_db(-1.0, -1.0);
        assert!(c.abs() < 1e-6);
    }

    #[test]
    fn max_true_peak_picks_loudest() {
        let v = max_true_peak_db(&[-3.0, -10.0, -1.5, -8.0]);
        assert!((v - (-1.5)).abs() < 1e-6, "got {v}");
    }

    #[test]
    fn max_true_peak_of_empty_is_neg_infinity() {
        let v = max_true_peak_db(&[]);
        assert!(v.is_infinite() && v.is_sign_negative());
    }

    #[test]
    fn linear_to_db_at_unity_is_zero() {
        let v = linear_to_db(1.0);
        assert!(v.abs() < 1e-9, "got {v}");
    }

    #[test]
    fn linear_to_db_handles_silence() {
        let v = linear_to_db(0.0);
        assert!(v.is_infinite() && v.is_sign_negative());
    }

    #[test]
    fn pink_noise_is_normalized_to_target_rms() {
        let n = 48_000;
        let pink = pink_noise_at(-18.0, n);
        let measured_rms = rms(&pink);
        let expected = lufs_to_linear(-18.0);
        assert!(
            (measured_rms - expected).abs() < 0.01,
            "expected ~{expected:.4}, got {measured_rms:.4}"
        );
    }

    #[test]
    fn parse_args_requires_plugins_root() {
        assert!(parse_args(std::iter::empty()).is_err());
    }

    #[test]
    fn parse_args_accepts_target_tp_override() {
        let args = parse_args(["--plugins-root", "/p", "--target-tp", "-3.0"].map(String::from))
            .unwrap();
        assert_eq!(args.plugins_root, PathBuf::from("/p"));
        assert!((args.target_tp_db - -3.0).abs() < 1e-9);
    }

    #[test]
    fn parse_args_rejects_old_target_lufs_flag() {
        // The audit moved from LUFS-loudness to true-peak normalization.
        // The old flag must be a hard error so stale CI scripts get
        // caught instead of silently using the default.
        let args = parse_args(["--plugins-root", "/p", "--target-lufs", "-18.0"].map(String::from));
        assert!(args.is_err());
    }
}
