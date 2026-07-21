use application::command::Command;
use application::dispatcher::CommandDispatcher;

use crate::state::{AppConfigYaml, ProjectPaths, ProjectSession};
use crate::AppWindow;
use anyhow::Result;
use domain::ids::DeviceId;
use infra_filesystem::{AppConfig, FilesystemStorage, GuiAudioDeviceSettings};
use infra_yaml::{load_chain_preset_file, ChainBlocksPreset};
use project::device::DeviceSettings;
use project::project::Project;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

// Issue #792 split: recent-projects + path/name helpers live in
// project_ops_recents.rs. Re-exported so crate::project_ops::* and the
// super:: paths in the persistence test modules keep resolving.
pub(crate) use crate::project_ops_recents::{
    canonical_project_path, mark_recent_project_invalid, parse_path_argument, project_display_name,
    recent_project_items, register_recent_project, sync_recent_projects,
};

pub(crate) fn open_cli_project(path: &PathBuf) -> Result<ProjectSession> {
    if !path.exists() {
        anyhow::bail!("CLI project path does not exist: {:?}", path);
    }
    let config_path = resolve_project_config_path(path);
    load_project_session(path, &config_path)
}

pub(crate) fn resolve_project_paths() -> ProjectPaths {
    ProjectPaths {
        default_config_path: parse_path_argument("--config").unwrap_or_else(|| {
            let local = PathBuf::from("config.yaml");
            if local.exists() {
                local
            } else {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config.yaml")
            }
        }),
    }
}

pub(crate) fn load_and_sync_app_config() -> Result<AppConfig> {
    let mut config = FilesystemStorage::load_app_config().unwrap_or_default();
    let changed = sync_recent_projects(&mut config);
    if changed {
        // #693: boot-time migration write goes to the persist worker.
        // #731: bind the config path at dispatch time.
        application::app_config_persist::persist_app_config_snapshot(config.clone());
    }
    Ok(config)
}

/// Default location for the bundled preset library.
///
/// Resolves to `<data_root>/presets` where `data_root` is:
/// - `<bundle>/Contents/Resources/` on macOS (.dmg / .app)
/// - `/usr/share/openrig/` on Linux (.deb / .rpm)
/// - `<install_dir>/` on Windows (.msi)
/// - the current working directory in dev (so `./presets` in the repo still works).
///
/// Used as the fallback when `config.yaml` has no `presets_path` entry; user
/// projects can still override this by setting `presets_path` in their own
/// `config.yaml`.
fn default_presets_path() -> PathBuf {
    infra_filesystem::detect_data_root().join("presets")
}

pub(crate) fn create_new_project_session(default_config_path: &Path) -> ProjectSession {
    let config = if default_config_path.exists() {
        load_app_config(default_config_path).unwrap_or_default()
    } else {
        AppConfigYaml {
            presets_path: Some(default_presets_path()),
        }
    };

    // #716 Task 20 (O4): auto-create the "default" I/O binding from the
    // system default input/output devices when opening a brand-new project.
    // This is idempotent — if a "default" binding already exists it is reused.
    ensure_default_io_binding(default_config_path);

    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    };
    let mut session = ProjectSession::new(
        project,
        None,
        None,
        config.presets_path.unwrap_or_else(default_presets_path),
    );
    // Attach an empty rig from the start so `Command::AddChain` can
    // mirror new chains into it (input + "Preset 1" + scene 1) without
    // waiting for a save/reload cycle. The GUI's preset combobox binds
    // against `session.rig`, so missing this leaves the combobox empty
    // until the project is saved and reopened.
    let rig = std::rc::Rc::new(std::cell::RefCell::new(project::rig::RigProject {
        name: None,
        inputs: std::collections::BTreeMap::new(),
        outputs: std::collections::BTreeMap::new(),
        presets: std::collections::BTreeMap::new(),
        midi: None,
        chain_order: Vec::new(),
    }));
    session.dispatcher.attach_rig(std::rc::Rc::clone(&rig));
    session.rig = Some(rig);
    // #716: hand the (possibly just-created) io_bindings registry to the
    // session so a new project's bound chains route per binding from the start.
    if let Ok(app_config) = FilesystemStorage::load_app_config() {
        *session.io_bindings.borrow_mut() = app_config.io_bindings;
    }
    session
}

pub(crate) fn load_app_config(path: &Path) -> Result<AppConfigYaml> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
}

pub(crate) fn resolve_project_config_path(project_path: &Path) -> PathBuf {
    project_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("config.yaml")
}

pub(crate) fn build_device_settings_from_gui(
    input_devices: &[GuiAudioDeviceSettings],
    output_devices: &[GuiAudioDeviceSettings],
) -> Vec<DeviceSettings> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for g in input_devices.iter().chain(output_devices.iter()) {
        if seen.insert(g.device_id.clone()) {
            result.push(DeviceSettings {
                device_id: DeviceId(g.device_id.clone()),
                sample_rate: g.sample_rate,
                buffer_size_frames: g.buffer_size_frames,
                bit_depth: g.bit_depth,
                #[cfg(target_os = "linux")]
                realtime: g.realtime,
                #[cfg(target_os = "linux")]
                rt_priority: g.rt_priority,
                #[cfg(target_os = "linux")]
                nperiods: g.nperiods,
            });
        }
    }
    result
}

/// #436 #1: load any project (new `project.openrig` or legacy `*.yaml`,
/// migrated transparently) through the NEW rig engine, projecting the
/// enabled inputs onto synthetic legacy chains so the existing GUI and
/// the proven cpal/runtime path drive the rig with zero new audio code.
/// Preset/scene switching has no UI yet (front deferred) — the rest
/// behaves exactly as before.
pub(crate) fn load_rig_and_project(
    project_path: &Path,
) -> Result<(project::rig::RigProject, Project)> {
    // `load_project_any` returns a validated RigProject (legacy `*.yaml`
    // migrated transparently). Every input is projected as a chain, all
    // OFF: the user enables what they want at runtime via the existing
    // per-chain toggle — nothing auto-starts. The RigProject is returned
    // so the session can keep it for preset/scene switching.
    let rig = infra_yaml::load_project_any(project_path)?;
    let project =
        engine::rig_runtime::rig_to_legacy_project(&rig, &std::collections::BTreeSet::new());
    Ok((rig, project))
}

pub(crate) fn load_project_session(
    project_path: &Path,
    config_path: &Path,
) -> Result<ProjectSession> {
    log::info!("loading project session from {:?}", project_path);
    let config = if config_path.exists() {
        load_app_config(config_path)?
    } else {
        AppConfigYaml::default()
    };
    let presets_path = config
        .presets_path
        .clone()
        .unwrap_or_else(default_presets_path);
    // #436 #1: the app now runs the new rig engine. Legacy `*.yaml` is
    // migrated transparently to `project.openrig` on first open. The
    // `RigProject` is retained in the session so the chains screen can
    // switch preset/scene per input.
    let (rig, mut project) = load_rig_and_project(project_path)?;

    // Populate device_settings from per-machine config (gui-settings.yaml)
    // instead of the project YAML. Old projects may still have device_settings
    // in their YAML — those are read for backward compat but overridden here.
    let gui_settings = FilesystemStorage::load_gui_audio_settings()
        .ok()
        .flatten()
        .unwrap_or_default();
    project.device_settings =
        build_device_settings_from_gui(&gui_settings.input_devices, &gui_settings.output_devices);

    // Migration safety net (#511 / output-persistence fix follow-up):
    // a rig-backed project saved before `SaveChainOutputEndpoints` started
    // writing into `rig.outputs` reopens with no Output blocks on its
    // chains. `validate_project` would then refuse to start the runtime
    // and the user would have no sound AND no way to enable the chain.
    // Model A (#716): a chain's output comes from the per-machine I/O binding
    // registry, not a synthesized device block — nothing to "ensure" here.

    // #606: the plugin catalog is loaded at startup, so by now we can tell
    // which block models resolve. Disable any whose pack is not installed
    // (or that is unsupported on this platform) — the chain keeps playing
    // with the pedal visibly off instead of silently faulting an "on" block.
    let disabled = project::project_disable_unavailable::disable_unavailable_blocks(&mut project);
    if !disabled.is_empty() {
        log::warn!(
            "disabled {} block(s) with unavailable models on load: {:?}",
            disabled.len(),
            disabled.iter().map(|b| &b.0).collect::<Vec<_>>()
        );
    }

    // #716: clean break from the old project format. Routing is binding-only —
    // the per-machine io_bindings registry (config.yaml) is the single source
    // of truth for I/O. There is NO legacy-entries migration: a legacy project
    // (Input/Output blocks with `entries` but empty `io`) opens UNBOUND and
    // must be reconfigured via the registry. Hand the existing registry to the
    // session so the live runtime resolves bound chains per binding.
    let registry_bindings: Vec<infra_filesystem::IoBinding> = FilesystemStorage::load_app_config()
        .map(|cfg| cfg.io_bindings)
        .unwrap_or_default();

    let mut session = ProjectSession::new(
        project,
        Some(project_path.to_path_buf()),
        Some(config_path.to_path_buf()),
        project_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(presets_path),
    );
    *session.io_bindings.borrow_mut() = registry_bindings;
    let rig = std::rc::Rc::new(std::cell::RefCell::new(rig));
    // #436: the dispatcher owns the rig so rig-nav goes through Command
    // (GUI/MIDI/MCP share one path). Same Rc the GUI renders from.
    session.dispatcher.attach_rig(std::rc::Rc::clone(&rig));
    session.rig = Some(rig);

    // #591: default the active chain to the first one. A footswitch bound
    // to `toggle_active_chain_enabled` reads `SelectionState.active_chain`;
    // with no prior navigation that was `None` and the press was a silent
    // no-op. Seeding it here also gives the Chains screen a chain to mark
    // the moment a project opens.
    let first_chain = session
        .project
        .borrow()
        .chains
        .first()
        .map(|c| c.id.clone());
    if let Some(first_chain) = first_chain {
        let _ = session
            .dispatcher
            .dispatch(application::command::Command::SelectActiveChain { chain: first_chain });
    }

    Ok(session)
}

/// The dirty-detection fingerprint. For a rig session the saved artifact
/// is the `.openrig` (the `RigProject`), so the fingerprint MUST include
/// it — switching preset/scene or editing sources often projects an
/// identical legacy `Project` (e.g. a scene with no overrides), so a
/// legacy-only snapshot would never flip dirty and Save would never be
/// offered ("cliquei numa scene e não deu opção de salvar"). Pure.
pub(crate) fn dirty_snapshot(
    project: &project::project::Project,
    rig: Option<&project::rig::RigProject>,
) -> Result<String> {
    let legacy = infra_yaml::serialize_project(project)?;
    match rig {
        Some(rig) => Ok(format!(
            "{legacy}\n---openrig---\n{}",
            infra_yaml::serialize_rig_project(rig)?
        )),
        None => Ok(legacy),
    }
}

pub(crate) fn project_session_snapshot(session: &ProjectSession) -> Result<String> {
    let rig = session.rig.as_ref().map(|r| r.borrow());
    dirty_snapshot(&session.project.borrow(), rig.as_deref())
}

pub(crate) fn set_project_dirty(
    window: &AppWindow,
    project_dirty: &std::rc::Rc<std::cell::RefCell<bool>>,
    dirty: bool,
) {
    *project_dirty.borrow_mut() = dirty;
    window.set_project_dirty(dirty);
}

#[track_caller]
pub(crate) fn sync_project_dirty(
    window: &AppWindow,
    session: &ProjectSession,
    saved_project_snapshot: &std::rc::Rc<std::cell::RefCell<Option<String>>>,
    project_dirty: &std::rc::Rc<std::cell::RefCell<bool>>,
    auto_save: bool,
) {
    if auto_save {
        if let Some(ref path) = session.project_path {
            // #555: auto-save goes through the dispatcher too — the
            // file writes live inside `Command::SaveProject`. Keep the
            // local snapshot fingerprint up to date so the next
            // dirty-check is accurate.
            match session.dispatcher.dispatch(Command::SaveProject) {
                Ok(_) => {
                    *saved_project_snapshot.borrow_mut() = project_session_snapshot(session).ok();
                    set_project_dirty(window, project_dirty, false);
                    log::debug!("auto-save: saved to {:?}", path);
                    return;
                }
                Err(e) => log::error!("auto-save failed: {e}"),
            }
        }
    }
    let dirty = match saved_project_snapshot.borrow().as_ref() {
        Some(saved_snapshot) => project_session_snapshot(session)
            .map(|current| current != *saved_snapshot)
            .unwrap_or(true),
        None => true,
    };
    set_project_dirty(window, project_dirty, dirty);
}

/// #555: test-only shim that dispatches `Command::SaveProject` after
/// attaching the session's paths. Production callers go through
/// `session.dispatcher.dispatch(Command::SaveProject)` directly —
/// this shim exists so the existing `project_ops_persistence_tests`
/// suite keeps exercising the end-to-end save path without each
/// test repeating the four attach + dispatch lines.
#[cfg(test)]
pub(crate) fn save_project_session(
    session: &ProjectSession,
    project_path: &std::path::Path,
) -> Result<()> {
    session
        .dispatcher
        .attach_project_path(project_path.to_path_buf());
    session
        .dispatcher
        .attach_presets_path(session.presets_path.clone());
    session
        .dispatcher
        .attach_config_path(session.config_path.clone());
    let result = session
        .dispatcher
        .dispatch(Command::SaveProject)
        .map(|_| ());
    // #693: writes are queued to the persist worker; the round-trip
    // suites reload right after saving, so wait for durability here.
    application::persist_worker::flush();
    result
}

// `save_chain_blocks_to_preset` was moved to
// `application::local_dispatcher_preset::handle_chain_preset` in #555.
// The GUI now dispatches `Command::SaveChainPreset { chain, name }`
// and the dispatcher does the file write.

pub(crate) fn load_preset_file(path: &Path) -> Result<ChainBlocksPreset> {
    load_chain_preset_file(path)
}

// `preset_id_from_path` lives inside `local_dispatcher_preset` now —
// the file id is derived at write time from the path the dispatcher
// resolves, not from a GUI helper.

/// #716 Task 20: ensure the `"default"` I/O binding exists in the AppConfig at
/// `config_path`. If the binding is already present this is a no-op (idempotent).
/// If the config carries at least one input and one output device, a binding is
/// built from the first of each and persisted synchronously (new-project creation
/// is not on the audio thread, so a direct write is fine here).
fn ensure_default_io_binding(config_path: &Path) {
    use crate::default_io_binding::{build_default_io_binding, DEFAULT_BINDING_ID};

    // Load the full AppConfig from the given path (not the OS global path).
    let raw = match fs::read_to_string(config_path) {
        Ok(r) => r,
        Err(_) => return, // Config does not exist yet — no devices to bind.
    };
    let mut app_config: AppConfig = match serde_yaml::from_str(&raw) {
        Ok(c) => c,
        Err(_) => return, // Malformed config — leave it alone.
    };

    // Idempotent: do not add a second "default" binding.
    if app_config
        .io_bindings
        .iter()
        .any(|b| b.id == DEFAULT_BINDING_ID)
    {
        return;
    }

    let input_id = match app_config.input_devices.first() {
        Some(d) => d.device_id.clone(),
        None => return, // No input device configured — cannot build binding.
    };
    let output_id = match app_config.output_devices.first() {
        Some(d) => d.device_id.clone(),
        None => return, // No output device configured — cannot build binding.
    };

    let binding = build_default_io_binding(&input_id, &output_id);
    app_config.io_bindings.push(binding);

    if let Ok(serialized) = serde_yaml::to_string(&app_config) {
        let _ = fs::write(config_path, serialized);
    }
}

pub(crate) fn project_title_for_path(project_path: Option<&PathBuf>, project: &Project) -> String {
    if let Some(name) = project
        .name
        .as_ref()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
    {
        return name.to_string();
    }
    project_path
        .and_then(|path| path.file_stem())
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| {
            if project.chains.is_empty() {
                "Novo Projeto".to_string()
            } else {
                "Projeto".to_string()
            }
        })
}

#[cfg(test)]
#[path = "project_ops_persistence_tests.rs"]
mod project_ops_persistence_tests;

#[cfg(test)]
#[path = "project_ops_persistence_more_tests.rs"]
mod project_ops_persistence_more;

#[cfg(test)]
#[path = "project_admin_persistence_tests.rs"]
mod project_admin_persistence_tests;

#[cfg(test)]
#[path = "project_rig_persistence_tests.rs"]
mod project_rig_persistence_tests;

#[cfg(test)]
#[path = "project_chain_defaults_persistence_tests.rs"]
mod project_chain_defaults_persistence_tests;

#[cfg(test)]
#[path = "project_chain_inmemory_tests.rs"]
mod project_chain_inmemory_tests;

#[cfg(test)]
#[path = "chain_rename_persistence_tests.rs"]
mod chain_rename_persistence_tests;

#[cfg(test)]
#[path = "scene_param_persistence_tests.rs"]
mod scene_param_persistence_tests;

#[cfg(test)]
#[path = "issue_690_nam_gate_persistence_tests.rs"]
mod issue_690_nam_gate_persistence_tests;

#[cfg(test)]
#[path = "chain_reorder_refresh_tests.rs"]
mod chain_reorder_refresh_tests;
