// Snapshot of complexity debt that existed on develop before the
// #548 build break was fixed (issue #576). Refactor of long fns and
// complex types is tracked under god-file ticket #276 and follow-ups.
// Allowing crate-wide keeps the QG honest about NEW regressions
// instead of perpetually re-reporting the existing snapshot.
#![allow(clippy::too_many_lines)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

use anyhow::Result;
use application::bridge::{self, QueryKind};
use application::dispatcher::CommandDispatcher;
use application::live_source::LiveSource;
use application::local_dispatcher::LocalDispatcher;
use application::publishing_dispatcher::PublishingDispatcher;
use application::read::{resolve, ReadContext};
use application::validate::validate_project;
use cpal::traits::StreamTrait;
use engine::runtime::build_runtime_graph;
use infra_cpal::{build_streams_for_project, list_devices, resolve_project_chain_sample_rates};
use infra_yaml::YamlProjectRepository;
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

mod tick;

/// The console's own [`LiveSource`] (#829/#831): it hosts a real device
/// enumeration and a real per-project sample rate, and nothing else — no
/// meters, analyzer, DI loop, or looper transport. The console drives the
/// engine directly and owns none of those runtimes, so every other read
/// falls through to the resolver's documented empty shape.
struct ConsoleLiveSource<'a> {
    project: &'a Project,
    io_bindings: &'a [domain::io_binding::IoBinding],
}

impl LiveSource for ConsoleLiveSource<'_> {
    // A dead audio host must surface as a failure, not an empty listing —
    // collapsing the two would hide a real problem behind "no devices to
    // report". Preserves today's console behavior exactly.
    fn devices(&self) -> Option<Result<Vec<String>, String>> {
        Some(list_devices().map_err(|e| e.to_string()))
    }

    // #723: the console has no live audio thread to ask, but it CAN resolve
    // the real per-chain device rate the same way `build_streams` does. A
    // single scalar cannot honestly stand in for every chain when chains
    // disagree (different devices, different rates — see the stream
    // isolation law), so this only answers when every enabled chain agrees;
    // otherwise `None` lets the resolver fall through to the dispatcher's
    // own tracked engine rate instead of this method lying.
    fn sample_rate(&self) -> Option<u32> {
        let rates = resolve_project_chain_sample_rates(self.project, self.io_bindings).ok()?;
        let mut values = rates.values().copied();
        let first = values.next()?;
        values.all(|r| r == first).then(|| first.round() as u32)
    }
}

/// Resolve one read through the shared core resolver (#829/#831) — the
/// console supplies only the two live sources it actually hosts; every
/// other read answers the documented empty shape or `NO_RIG_ATTACHED`,
/// exactly like every other transport. Factored out (rather than inlined
/// in the event loop) so `console_read_tests` exercises the exact wiring
/// the loop runs.
fn console_resolve(
    kind: &QueryKind,
    project: &Project,
    io_bindings: &[domain::io_binding::IoBinding],
    dispatcher: &dyn CommandDispatcher,
) -> Result<String, String> {
    resolve(
        kind,
        &ReadContext {
            project,
            // #554: the console speaks the device-level engine directly and
            // attaches no `RigProject` — the preset reads answer the shared
            // `NO_RIG_ATTACHED` failure, same as every transport with no rig.
            rig: None,
            io_bindings,
            dispatcher,
            live: &ConsoleLiveSource {
                project,
                io_bindings,
            },
        },
    )
}

#[cfg(test)]
#[path = "console_read_tests.rs"]
mod console_read_tests;

#[derive(Debug, Deserialize, Default)]
struct AppConfigYaml {
    #[serde(default, rename = "presets_path")]
    _presets_path: Option<PathBuf>,
}

/// Build the runtime graph + streams for the current project state.
fn build_streams(project: &Project) -> Result<Vec<cpal::Stream>> {
    // Model A (#716): device I/O comes from the per-machine binding registry.
    let registry = infra_filesystem::FilesystemStorage::load_app_config()
        .map(|c| c.io_bindings)
        .unwrap_or_default();
    let rates = resolve_project_chain_sample_rates(project, &registry)?;
    let graph = build_runtime_graph(project, &rates, &HashMap::new(), &registry)?;
    let streams = build_streams_for_project(project, &graph, &registry)?;
    for stream in &streams {
        stream.play()?;
    }
    Ok(streams)
}

fn main() -> Result<()> {
    // Issue #670: without a logger every log::* line (worker timing, stream
    // diagnostics) was silently dropped — a clean-looking run proved nothing.
    env_logger::init();
    let project_path = parse_project_path();
    let config_path = parse_config_path();
    let _config = load_app_config(&config_path)?;
    let mcp_addr = parse_mcp_addr();
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    // Issue #670: the console never initialized the plugin catalog, so every
    // NAM/IR/LV2 block in a real rig was silently dropped as "unsupported"
    // (the chain degraded to passthrough — useless for validating real
    // presets headless). Mirror the GUI's startup registration: natives
    // first, then the bundled + user plugin roots.
    let bundled_root = infra_filesystem::detect_data_root().join("plugins");
    let user_root = plugin_loader::plugins_root_from_config(&config_path);
    engine::native_registry::register_all_natives();
    plugin_loader::registry::init_many(&[bundled_root, user_root]);
    println!(
        "plugins: {} loaded ({} native)",
        plugin_loader::registry::len(),
        plugin_loader::registry::native_count(),
    );
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
        let changed = !tick::tick(&dispatcher, drain.drain(&dispatcher, 64)).is_empty();
        let io_bindings = infra_filesystem::FilesystemStorage::load_app_config()
            .map(|c| c.io_bindings)
            .unwrap_or_default();
        drain.serve_queries(
            |kind| {
                let project = shared.borrow();
                console_resolve(kind, &project, &io_bindings, dispatcher.inner())
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
    let args = env::args().skip(1);
    for arg in args {
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
    let args = env::args().skip(1);
    for arg in args {
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
