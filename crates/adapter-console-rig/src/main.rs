//! Headless rig runner (#436 / #1 integration).
//!
//! Loads a `project.openrig` (or transparently migrates a legacy
//! `*.yaml` via `load_project_any`), builds a `RigRuntime` (validates +
//! auto-enables every non-tap-conflicting input), projects the enabled
//! inputs onto synthetic legacy chains, and drives them through the
//! EXISTING proven cpal path (`build_runtime_graph` /
//! `build_streams_for_project`). No UI, no new audio code — so every
//! audio invariant the legacy path holds is held here by construction.
//!
//!   cargo run -p adapter-console-rig -- --project /path/project.openrig
//!   cargo run -p adapter-console-rig -- --project /path/legacy.yaml   # auto-migrates

use anyhow::Result;
use application::validate::validate_project;
use cpal::traits::StreamTrait;
use engine::rig_runtime::{rig_to_legacy_project, RigRuntime};
use engine::runtime::build_runtime_graph;
use infra_cpal::{build_streams_for_project, list_devices, resolve_project_chain_sample_rates};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;

fn main() -> Result<()> {
    let project_path = parse_project_path()?;
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());

    // Transparent: new `.openrig` as-is, or legacy `*.yaml` auto-migrated
    // (writes a sibling `.openrig` + one-time `.bak`).
    let rig = infra_yaml::load_project_any(&project_path)?;
    // Model A (#716): device I/O comes from the per-machine binding registry
    // (system config), never from the project. Load it; empty if unconfigured.
    let registry = infra_filesystem::FilesystemStorage::load_app_config()
        .map(|c| c.io_bindings)
        .unwrap_or_default();
    println!("=== Rig ===");
    println!(
        "name={:?} inputs={} presets={} outputs={}",
        rig.name,
        rig.inputs.len(),
        rig.presets.len(),
        rig.outputs.len()
    );

    // Validates the project and auto-enables every input whose
    // (device, channel) tap is free (invariant #4).
    let runtime = RigRuntime::build(rig, DEFAULT_SAMPLE_RATE, registry.clone())?;
    let proj = runtime.project();
    let enabled: BTreeSet<String> = proj
        .inputs
        .keys()
        .filter(|name| runtime.is_enabled(name))
        .cloned()
        .collect();
    println!(
        "enabled inputs ({}/{}): {}",
        enabled.len(),
        proj.inputs.len(),
        enabled.iter().cloned().collect::<Vec<_>>().join(", ")
    );

    let legacy = rig_to_legacy_project(proj, &enabled);
    validate_project(&legacy)?;

    println!("=== Devices ===");
    for line in list_devices()? {
        println!("{line}");
    }

    let chain_sample_rates = resolve_project_chain_sample_rates(&legacy, &registry)?;
    let runtime_graph = build_runtime_graph(&legacy, &chain_sample_rates, &HashMap::new(), &registry)?;
    let streams = build_streams_for_project(&legacy, &runtime_graph, &registry)?;
    for stream in &streams {
        stream.play()?;
    }

    println!("=== Engine ===");
    println!("running=true active_inputs={}", enabled.len());
    println!("Ctrl-C to stop.");
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn parse_project_path() -> Result<PathBuf> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--project" {
            if let Some(path) = args.next() {
                return Ok(PathBuf::from(path));
            }
        }
    }
    anyhow::bail!("usage: adapter-console-rig --project <path to .openrig or legacy .yaml>")
}
