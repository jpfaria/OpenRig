//! Loudness audit for NAM plugin packages (issue #402, phase 3).
//!
//! Walks `<plugins-root>/nam/<id>/`, runs each capture against a pink-noise
//! reference signal, measures integrated LUFS, and writes the
//! corrective `output_gain_db` field into the package's `manifest.yaml`.
//!
//! After running this tool, every NAM in the catalogue lands at the same
//! perceived loudness when first added to a chain. Users can still
//! override per preset via `params.output_db` (Phase 1).
//!
//! Usage:
//!
//! ```text
//! nam_loudness_audit --plugins-root <path> [--target-lufs -18.0]
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

const DEFAULT_TARGET_LUFS: f64 = -18.0;
const SAMPLE_RATE: f32 = 48_000.0;
const MEASURE_SECONDS: usize = 10;

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
        match audit_one_package(&package_dir, args.target_lufs) {
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
    target_lufs: f64,
}

fn parse_args<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut plugins_root: Option<PathBuf> = None;
    let mut target_lufs: f64 = DEFAULT_TARGET_LUFS;
    let mut iter = iter.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--plugins-root" => {
                let v = iter
                    .next()
                    .ok_or_else(|| anyhow!("--plugins-root needs a path"))?;
                plugins_root = Some(PathBuf::from(v));
            }
            "--target-lufs" => {
                let v = iter
                    .next()
                    .ok_or_else(|| anyhow!("--target-lufs needs a value"))?;
                target_lufs = v.parse().context("invalid --target-lufs")?;
            }
            "--help" | "-h" => {
                println!(
                    "Usage: nam_loudness_audit --plugins-root <path> \
                     [--target-lufs {DEFAULT_TARGET_LUFS}]"
                );
                std::process::exit(0);
            }
            unknown => bail!("unknown argument `{unknown}`"),
        }
    }
    Ok(Args {
        plugins_root: plugins_root.ok_or_else(|| anyhow!("--plugins-root is required"))?,
        target_lufs,
    })
}

#[derive(Default)]
struct AuditReport {
    audited: Vec<AuditOutcome>,
    failed: Vec<AuditFailure>,
}

struct AuditOutcome {
    id: String,
    measured_lufs: f64,
    correction_db: f32,
    previous_correction_db: Option<f32>,
}

struct AuditFailure {
    package: PathBuf,
    error: String,
}

fn audit_one_package(package_dir: &Path, target_lufs: f64) -> Result<AuditOutcome> {
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
        .map(|cap| measure_capture_lufs(package_dir, &cap.file))
        .collect::<Result<_>>()?;
    let measured_lufs = average_lufs(&measurements);
    let correction_db = compute_correction_db(measured_lufs, target_lufs);
    let previous_correction_db = manifest.output_gain_db;
    manifest.output_gain_db = Some(correction_db);
    let serialized =
        serde_yaml::to_string(&manifest).context("re-serialize manifest with output_gain_db")?;
    fs::write(&manifest_path, serialized)
        .with_context(|| format!("write {}", manifest_path.display()))?;
    Ok(AuditOutcome {
        id: manifest.id,
        measured_lufs,
        correction_db,
        previous_correction_db,
    })
}

fn measure_capture_lufs(package_dir: &Path, capture_file: &Path) -> Result<f64> {
    let model_path = package_dir.join(capture_file);
    let model_path_str = model_path
        .to_str()
        .ok_or_else(|| anyhow!("non-utf8 capture path: {model_path:?}"))?;
    let params = ParameterSet::default();
    let plugin_params = nam::from_package::effective_plugin_params(
        &dummy_manifest_for_capture(model_path_str),
        &params,
    )?;
    let mut processor = nam::build_processor_with_assets_for_layout(
        model_path_str,
        None,
        plugin_params,
        SAMPLE_RATE,
        AudioChannelLayout::Mono,
    )?;
    let n_samples = (SAMPLE_RATE as usize) * MEASURE_SECONDS;
    let pink = pink_noise_at(-18.0, n_samples);
    let mut output = Vec::with_capacity(n_samples);
    for sample in pink.iter() {
        match &mut processor {
            block_core::BlockProcessor::Mono(p) => output.push(p.process_sample(*sample)),
            block_core::BlockProcessor::Stereo(_) => {
                bail!("expected mono processor for capture")
            }
        }
    }
    let mut meter = EbuR128::new(1, SAMPLE_RATE as u32, Mode::I)
        .map_err(|e| anyhow!("ebur128 init: {e}"))?;
    meter
        .add_frames_f32(&output)
        .map_err(|e| anyhow!("ebur128 add_frames: {e}"))?;
    meter
        .loudness_global()
        .map_err(|e| anyhow!("ebur128 loudness_global: {e}"))
}

/// Compute the dB correction needed to land at `target_lufs` from a
/// measured `measured_lufs`. Pure function, no I/O.
pub fn compute_correction_db(measured_lufs: f64, target_lufs: f64) -> f32 {
    (target_lufs - measured_lufs) as f32
}

/// Average loudness across captures within a package. Used as the
/// per-package single correction value, since one NAM bundle can ship
/// multiple variants and they should all sit at the same loudness.
pub fn average_lufs(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let sum: f64 = values.iter().sum();
    sum / values.len() as f64
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

fn dummy_manifest_for_capture(_path: &str) -> PluginManifest {
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
            "  ok    {:32}  measured {:+6.2} LUFS  correction {:+6.2} dB  (was {})",
            outcome.id, outcome.measured_lufs, outcome.correction_db, prev
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
        // Measured 6 dB louder than target -> correction must be -6.
        let c = compute_correction_db(-12.0, -18.0);
        assert!((c - (-6.0)).abs() < 1e-6, "got {c}");
    }

    #[test]
    fn correction_zero_when_already_at_target() {
        let c = compute_correction_db(-18.0, -18.0);
        assert!(c.abs() < 1e-6);
    }

    #[test]
    fn correction_positive_when_too_quiet() {
        let c = compute_correction_db(-24.0, -18.0);
        assert!((c - 6.0).abs() < 1e-6, "got {c}");
    }

    #[test]
    fn average_of_empty_is_zero() {
        assert_eq!(average_lufs(&[]), 0.0);
    }

    #[test]
    fn average_matches_simple_mean() {
        let v = average_lufs(&[-18.0, -16.0, -20.0]);
        assert!((v - (-18.0)).abs() < 1e-6, "got {v}");
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
    fn parse_args_accepts_target_lufs_override() {
        let args =
            parse_args(["--plugins-root", "/p", "--target-lufs", "-23.0"].map(String::from))
                .unwrap();
        assert_eq!(args.plugins_root, PathBuf::from("/p"));
        assert!((args.target_lufs - -23.0).abs() < 1e-9);
    }
}
