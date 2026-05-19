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
    // one command bridge — multiple producers, single frontend drain.
    if let Some(map_path) = parse_midi_map()? {
        let bridge_for_midi = cmd_bridge.clone();
        let map_for_thread = map_path.clone();
        thread::Builder::new()
            .name("openrig-midi".into())
            .spawn(move || {
                if let Err(e) = adapter_midi::run_blocking(bridge_for_midi, &map_for_thread) {
                    eprintln!("MIDI adapter stopped: {e}");
                }
            })?;
        println!("=== MIDI === map {}", map_path.display());
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

/// `--midi` → per-OS default `midi-map.yaml`; `--midi=PATH` → that file;
/// absent → `None` (adapter not started).
fn parse_midi_map() -> Result<Option<PathBuf>> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--midi" {
            return Ok(Some(infra_filesystem::FilesystemStorage::midi_map_path()?));
        }
        if let Some(rest) = arg.strip_prefix("--midi=") {
            return Ok(Some(PathBuf::from(rest)));
        }
    }
    Ok(None)
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
