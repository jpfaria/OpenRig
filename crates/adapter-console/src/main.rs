use anyhow::Result;
use application::bridge::{self, QueryKind};
use application::local_dispatcher::LocalDispatcher;
use application::publishing_dispatcher::PublishingDispatcher;
use application::validate::validate_project;
use cpal::traits::StreamTrait;
use engine::runtime::build_runtime_graph;
use infra_cpal::{build_streams_for_project, list_devices, resolve_project_chain_sample_rates};
use infra_yaml::{serialize_project, YamlProjectRepository};
use project::project::Project;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::rc::Rc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Deserialize, Default)]
struct AppConfigYaml {
    #[serde(default, rename = "presets_path")]
    _presets_path: Option<PathBuf>,
}

/// Build the runtime graph + streams for the current project state.
fn build_streams(project: &Project) -> Result<Vec<cpal::Stream>> {
    let rates = resolve_project_chain_sample_rates(project)?;
    let graph = build_runtime_graph(project, &rates, &HashMap::new())?;
    let streams = build_streams_for_project(project, &graph)?;
    for stream in &streams {
        stream.play()?;
    }
    Ok(streams)
}

fn main() -> Result<()> {
    let project_path = parse_project_path();
    let config_path = parse_config_path();
    let _config = load_app_config(&config_path)?;
    let mcp_addr = parse_mcp_addr();
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let project_repo = YamlProjectRepository { path: project_path };
    let project = project_repo.load_current_project()?;
    validate_project(&project)?;
    println!("=== Devices ===");
    for line in list_devices()? {
        println!("{line}");
    }
    println!("=== Project ===");
    println!("chains={}", project.chains.len());

    // Shared project handle: the dispatcher and this loop see the same data.
    let shared = Rc::new(RefCell::new(project));
    let (sink, _events_rx) = bridge::event_sink();
    let dispatcher = PublishingDispatcher::new(LocalDispatcher::new(Rc::clone(&shared)), sink);
    let (cmd_bridge, drain) = bridge::channel();

    if let Some(addr) = mcp_addr {
        let bridge_for_mcp = cmd_bridge.clone();
        thread::Builder::new()
            .name("openrig-mcp".into())
            .spawn(move || {
                if let Err(e) = adapter_mcp::run_blocking(bridge_for_mcp, addr) {
                    eprintln!("MCP server stopped: {e}");
                }
            })?;
        println!("=== MCP === listening on http://{addr}");
    }

    // MIDI/BLE-MIDI controller adapter (opt-in, --midi[=PATH]). Reuses the
    // one command bridge — multiple producers, single frontend drain. With
    // `--midi` (no path), ADR 0003 / #499 resolves the runtime map from the
    // system layer (no project bindings here — console runs the legacy chain
    // model). `--midi=PATH` still loads the explicit legacy file directly.
    if let Some(arg) = parse_midi_map() {
        let bridge_for_midi = cmd_bridge.clone();
        match arg {
            MidiMapArg::Default => {
                let legacy = infra_filesystem::FilesystemStorage::midi_map_path()?;
                let profile_path = infra_filesystem::FilesystemStorage::midi_profile_path()?;
                let bindings_path = infra_filesystem::FilesystemStorage::midi_bindings_path()?;
                if let Err(e) = infra_filesystem::midi_migrate::migrate_legacy_midi_map(
                    &legacy,
                    &profile_path,
                    &bindings_path,
                ) {
                    eprintln!("legacy midi-map.yaml migration failed: {e}");
                }
                let profile =
                    infra_filesystem::midi_profile::MidiDeviceProfile::load(&profile_path)?;
                let shipped_default =
                    infra_filesystem::detect_data_root().join("examples/midi-map.default.yaml");
                let map = adapter_midi::resolve_midi_map(
                    None,
                    &profile,
                    &bindings_path,
                    &shipped_default,
                )?;
                println!(
                    "=== MIDI === resolved: input={:?}, bindings={}",
                    map.input,
                    map.bindings.len()
                );
                // #513 / #493: console has no learn UI but the daemon still
                // needs the flag handle (off by default — same observable
                // behaviour as before).
                let learn = adapter_midi::learn_state();
                thread::Builder::new()
                    .name("openrig-midi".into())
                    .spawn(move || {
                        if let Err(e) =
                            adapter_midi::run_blocking_with_map(bridge_for_midi, map, learn)
                        {
                            eprintln!("MIDI adapter stopped: {e}");
                        }
                    })?;
            }
            MidiMapArg::Path(map_path) => {
                println!("=== MIDI === legacy map {}", map_path.display());
                let learn = adapter_midi::learn_state();
                thread::Builder::new()
                    .name("openrig-midi".into())
                    .spawn(move || {
                        if let Err(e) =
                            adapter_midi::run_blocking(bridge_for_midi, &map_path, learn)
                        {
                            eprintln!("MIDI adapter stopped: {e}");
                        }
                    })?;
            }
        }
    }

    // `streams` is RAII: kept bound for the whole loop so audio keeps running
    // (dropping a `cpal::Stream` stops it). Reassigned on a live rebuild.
    let mut streams = build_streams(&shared.borrow())?;
    println!(
        "=== Engine ===\nrunning=true active_chains={} streams={}",
        shared.borrow().chains.iter().filter(|c| c.enabled).count(),
        streams.len()
    );

    loop {
        let changed = !drain.drain(&dispatcher, 64).is_empty();
        drain.serve_queries(
            |kind| match kind {
                QueryKind::ProjectYaml => {
                    serialize_project(&shared.borrow()).map_err(|e| e.to_string())
                }
                QueryKind::Devices => list_devices()
                    .map(|d| d.join("\n"))
                    .map_err(|e| e.to_string()),
                QueryKind::Ids => Ok(application::query::list_ids(&shared.borrow())),
                QueryKind::ChainMeters => {
                    // Console adapter has no live meter source — emit a
                    // silent record per chain so the MCP resource shape
                    // is stable regardless of which adapter is mounted.
                    let proj = shared.borrow();
                    let mut out = String::new();
                    for chain in &proj.chains {
                        out.push_str(&format!("{}\t-120.0\t-120.0\n", chain.id.0));
                    }
                    Ok(out)
                }
                // #561 (expanded scope): plugin catalog reads — same
                // pure helpers MCP would call (process-wide registry).
                QueryKind::ListPluginCatalog => Ok(application::query::list_plugin_catalog()),
                QueryKind::GetPlugin { id } => Ok(application::query::get_plugin(id)),
                QueryKind::FindPlugins { query } => Ok(application::query::find_plugins(query)),
            },
            64,
        );
        if changed {
            // A command mutated the project: rebuild the live graph. On a
            // validation/build error keep the previous streams running.
            match validate_project(&shared.borrow()).and_then(|_| build_streams(&shared.borrow())) {
                Ok(new_streams) => {
                    streams = new_streams;
                    println!("runtime rebuilt: streams={}", streams.len());
                }
                Err(e) => eprintln!("runtime rebuild skipped: {e}"),
            }
        }
        thread::sleep(Duration::from_millis(16));
    }
}

fn parse_mcp_addr() -> Option<SocketAddr> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--mcp" {
            return Some("127.0.0.1:4123".parse().expect("default mcp addr"));
        }
        if let Some(rest) = arg.strip_prefix("--mcp=") {
            return rest.parse().ok().or_else(|| {
                eprintln!("invalid --mcp address: {rest}");
                None
            });
        }
    }
    None
}

/// `--midi` → resolved view per ADR 0003 / #499 (system profile + system
/// fallback bindings / shipped default); `--midi=PATH` → legacy direct file
/// load (no migration, no resolution); absent → adapter not started.
enum MidiMapArg {
    Default,
    Path(PathBuf),
}

fn parse_midi_map() -> Option<MidiMapArg> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--midi" {
            return Some(MidiMapArg::Default);
        }
        if let Some(rest) = arg.strip_prefix("--midi=") {
            return Some(MidiMapArg::Path(PathBuf::from(rest)));
        }
    }
    None
}

fn parse_project_path() -> PathBuf {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--project" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    let local_project = PathBuf::from("project.yaml");
    if local_project.exists() {
        return local_project;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../project.yaml")
}

fn parse_config_path() -> PathBuf {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    let local_config = PathBuf::from("config.yaml");
    if local_config.exists() {
        return local_config;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config.yaml")
}

fn load_app_config(path: &PathBuf) -> Result<AppConfigYaml> {
    if !path.exists() {
        return Ok(AppConfigYaml::default());
    }
    let raw = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
}
