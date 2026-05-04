mod classifier;
mod codegen;
mod model;
mod parser;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use model::Availability;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "lv2_catalog_import",
    about = "Auto-import LV2 bundles into OpenRig MODEL_DEFINITION catalog"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Scan bundles, classify and report — write nothing
    DryRun {
        #[arg(long, default_value = ".plugins/lv2")]
        plugins_root: PathBuf,
        #[arg(long, default_value = "tools/lv2_catalog_import/overrides.yaml")]
        overrides: PathBuf,
        #[arg(
            long,
            default_value = "tools/lv2_catalog_import/cross_platform_map.yaml"
        )]
        cross_map: PathBuf,
        #[arg(long)]
        bundle: Option<String>,
    },
    /// Generate lv2_*.rs files into crates/block-*/src/
    Apply {
        #[arg(long, default_value = ".plugins/lv2")]
        plugins_root: PathBuf,
        #[arg(long, default_value = "tools/lv2_catalog_import/overrides.yaml")]
        overrides: PathBuf,
        #[arg(
            long,
            default_value = "tools/lv2_catalog_import/cross_platform_map.yaml"
        )]
        cross_map: PathBuf,
        #[arg(long, default_value = "crates")]
        crates_root: PathBuf,
        #[arg(long)]
        bundle: Option<String>,
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::DryRun {
            plugins_root,
            overrides,
            cross_map,
            bundle,
        } => dry_run(&plugins_root, &overrides, &cross_map, bundle.as_deref()),
        Cmd::Apply {
            plugins_root,
            overrides,
            cross_map,
            crates_root,
            bundle,
            force,
        } => apply(
            &plugins_root,
            &overrides,
            &cross_map,
            &crates_root,
            bundle.as_deref(),
            force,
        ),
    }
}

fn dry_run(
    plugins_root: &Path,
    overrides_path: &Path,
    cross_map_path: &Path,
    only_bundle: Option<&str>,
) -> Result<()> {
    let overrides = classifier::load_overrides(overrides_path)
        .with_context(|| format!("load {}", overrides_path.display()))?;
    let cross_map = classifier::load_cross_map(cross_map_path)
        .with_context(|| format!("load {}", cross_map_path.display()))?;
    let bundles = parser::discover_bundles(plugins_root)
        .with_context(|| format!("discover {}", plugins_root.display()))?;

    let mut totals = BTreeMap::<String, usize>::new();
    let mut by_availability = BTreeMap::<String, usize>::new();
    let mut skipped = 0usize;
    let mut accepted = 0usize;
    let mut sample_per_block: BTreeMap<String, Vec<String>> = BTreeMap::new();

    println!("# LV2 catalog import — dry-run report");
    println!("scanned root: {}", plugins_root.display());
    println!();
    println!("## Per-bundle classification");

    for bundle in &bundles {
        if let Some(b) = only_bundle {
            if bundle.bundle_dir != b {
                continue;
            }
        }
        println!("\n### {}", bundle.bundle_dir);
        for plugin in &bundle.plugins {
            let cls = classifier::classify(plugin, &overrides, &cross_map);
            let block = cls
                .block_type
                .map(|b| b.crate_name().to_string())
                .unwrap_or_else(|| "—".to_string());
            let mode = cls
                .audio_mode
                .map(|m| format!("{:?}", m))
                .unwrap_or_else(|| "—".to_string());
            let avail = format!("{:?}", cls.availability);
            let badge = match cls.availability {
                Availability::Skip => "SKIP",
                Availability::LinuxOnly => "linux",
                Availability::Cross => "cross",
            };
            println!(
                "  - [{}] {} | block={} mode={} avail={} ai={}/ao={} classes={:?}{}",
                badge,
                plugin.uri,
                block,
                mode,
                avail,
                plugin.audio_in_count(),
                plugin.audio_out_count(),
                plugin.plugin_classes,
                cls.skip_reason
                    .as_ref()
                    .map(|r| format!(" reason='{r}'"))
                    .unwrap_or_default()
            );

            *totals.entry(block.clone()).or_default() += 1;
            *by_availability.entry(avail).or_default() += 1;
            if cls.skip_reason.is_some() {
                skipped += 1;
            } else {
                accepted += 1;
                sample_per_block
                    .entry(block)
                    .or_default()
                    .push(cls.model_id.clone());
            }
        }
    }

    println!("\n## Summary");
    println!("total bundles: {}", bundles.len());
    println!("plugins accepted: {}", accepted);
    println!("plugins skipped: {}", skipped);
    println!("\nby block-type:");
    for (k, v) in &totals {
        println!("  {:>14}  {}", k, v);
    }
    println!("\nby availability:");
    for (k, v) in &by_availability {
        println!("  {:>10}  {}", k, v);
    }
    println!("\nsample per block-type (first 5):");
    for (k, ids) in &sample_per_block {
        let head: Vec<&String> = ids.iter().take(5).collect();
        println!("  {}: {:?}", k, head);
    }
    Ok(())
}

fn apply(
    plugins_root: &Path,
    overrides_path: &Path,
    cross_map_path: &Path,
    crates_root: &Path,
    only_bundle: Option<&str>,
    force: bool,
) -> Result<()> {
    let overrides = classifier::load_overrides(overrides_path)
        .with_context(|| format!("load {}", overrides_path.display()))?;
    let cross_map = classifier::load_cross_map(cross_map_path)
        .with_context(|| format!("load {}", cross_map_path.display()))?;
    let bundles = parser::discover_bundles(plugins_root)
        .with_context(|| format!("discover {}", plugins_root.display()))?;

    let mut written = 0usize;
    let mut skipped = 0usize;
    let mut existing = 0usize;

    for bundle in &bundles {
        if let Some(b) = only_bundle {
            if bundle.bundle_dir != b {
                continue;
            }
        }
        for plugin in &bundle.plugins {
            let cls = classifier::classify(plugin, &overrides, &cross_map);
            let Some(block_type) = cls.block_type else {
                skipped += 1;
                continue;
            };
            let target_dir = crates_root.join(block_type.crate_name()).join("src");
            if !target_dir.exists() {
                eprintln!("warn: target dir missing: {}", target_dir.display());
                skipped += 1;
                continue;
            }
            let target_file = target_dir.join(format!("{}.rs", cls.model_id));
            if target_file.exists() {
                let is_autogen = std::fs::read_to_string(&target_file)
                    .map(|s| s.contains("AUTO-GENERATED by tools/lv2_catalog_import"))
                    .unwrap_or(false);
                if !is_autogen {
                    // Manual file — never overwrite, even with --force.
                    existing += 1;
                    continue;
                }
                if !force {
                    existing += 1;
                    continue;
                }
            }
            let Some(rendered) = codegen::render_model_def(plugin, &cls) else {
                skipped += 1;
                continue;
            };
            std::fs::write(&target_file, rendered)
                .with_context(|| format!("write {}", target_file.display()))?;
            written += 1;
            println!("wrote {}", target_file.display());
        }
    }

    println!(
        "\n=== apply done: written={} skipped={} existing={} ===",
        written, skipped, existing
    );
    Ok(())
}
