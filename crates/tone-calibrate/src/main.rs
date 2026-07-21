//! `openrig-tone-calibrate` — offline Tone Doctor limit calibration (#809).
//!
//! Usage:
//!   openrig-tone-calibrate <evaluations-root> <manifest.yaml> [out.yaml] [percentile]
//!   openrig-tone-calibrate measure <evaluations-root> <manifest.yaml> [out.csv]
//!
//! The default form reads the genre-labeled stems and writes the per-genre limit
//! table (stdout when no `out.yaml`). The `measure` form dumps the per-song,
//! per-stem raw descriptors as CSV (stdout when no `out.csv`) for charting.
//! `percentile` defaults to `feature_dsp::tone_profiles::DEFAULT_PERCENTILE`.

use anyhow::{Context, Result};
use feature_dsp::tone_profiles::DEFAULT_PERCENTILE;
use std::path::PathBuf;
use tone_calibrate::{calibrate_corpus, measure_stems, measurements_to_csv, to_yaml, Manifest};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next().context("missing <evaluations-root> (or `measure`)")?;
    if first == "measure" {
        return run_measure(&mut args);
    }
    let root: PathBuf = first.into();
    let manifest_path: PathBuf = args.next().context("missing <manifest.yaml>")?.into();
    let out_path = args.next().map(PathBuf::from);
    let percentile: f32 = match args.next() {
        Some(p) => p
            .parse()
            .context("percentile must be a number in 0.0..=1.0")?,
        None => DEFAULT_PERCENTILE,
    };

    let manifest: Manifest = serde_yaml::from_str(
        &std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading manifest {}", manifest_path.display()))?,
    )
    .context("parsing manifest YAML (expected a flat `song: genre` map)")?;

    let profiles = calibrate_corpus(&root, &manifest, percentile)?;
    let yaml = to_yaml(&profiles)?;

    match out_path {
        Some(path) => {
            std::fs::write(&path, &yaml).with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {} genres to {}", profiles.len(), path.display());
        }
        None => print!("{yaml}"),
    }
    Ok(())
}

/// `measure` subcommand: dump per-stem descriptors as CSV.
fn run_measure(args: &mut impl Iterator<Item = String>) -> Result<()> {
    let root: PathBuf = args.next().context("missing <evaluations-root>")?.into();
    let manifest_path: PathBuf = args.next().context("missing <manifest.yaml>")?.into();
    let out_path = args.next().map(PathBuf::from);

    let manifest: Manifest = serde_yaml::from_str(
        &std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading manifest {}", manifest_path.display()))?,
    )
    .context("parsing manifest YAML (expected a flat `song: genre` map)")?;

    let measurements = measure_stems(&root, &manifest)?;
    let csv = measurements_to_csv(&measurements);
    match out_path {
        Some(path) => {
            std::fs::write(&path, &csv).with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {} stems to {}", measurements.len(), path.display());
        }
        None => print!("{csv}"),
    }
    Ok(())
}
