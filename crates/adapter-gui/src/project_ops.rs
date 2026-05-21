use application::command::Command;
use application::dispatcher::CommandDispatcher;

use crate::state::{AppConfigYaml, ConfigYaml, ProjectPaths, ProjectSession};
use crate::RecentProjectItem;
use crate::{AppWindow, UNTITLED_PROJECT_NAME};
use anyhow::{anyhow, Result};
use domain::ids::DeviceId;
use infra_filesystem::{AppConfig, FilesystemStorage, GuiAudioDeviceSettings, RecentProjectEntry};
use infra_yaml::{load_chain_preset_file, save_chain_preset_file, ChainBlocksPreset};
use project::block::AudioBlockKind;
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
        let _ = FilesystemStorage::save_app_config(&config);
    }
    Ok(config)
}

pub(crate) fn sync_recent_projects(config: &mut AppConfig) -> bool {
    let original = config.clone();
    let mut synced = Vec::new();
    for recent in &config.recent_projects {
        let path = PathBuf::from(&recent.project_path);
        // Skip path.exists() check here — it can block indefinitely on
        // disconnected network volumes or external drives (macOS stat hang).
        // Validity is checked lazily when the user tries to open the project.
        let canonical_path = if path.is_absolute() {
            path.clone()
        } else {
            env::current_dir()
                .map(|d| d.join(&path))
                .unwrap_or(path.clone())
        };
        let canonical_path_string = canonical_path.to_string_lossy().to_string();
        if synced
            .iter()
            .any(|current: &RecentProjectEntry| current.project_path == canonical_path_string)
        {
            continue;
        }
        synced.push(RecentProjectEntry {
            project_path: canonical_path_string,
            project_name: if recent.project_name.trim().is_empty() {
                UNTITLED_PROJECT_NAME.to_string()
            } else {
                recent.project_name.clone()
            },
            is_valid: true,
            invalid_reason: None,
        });
    }
    config.recent_projects = synced;
    *config != original
}

pub(crate) fn canonical_project_path(path: &PathBuf) -> Result<PathBuf> {
    // Do NOT call path.exists() here — blocks on disconnected network volumes.
    // fs::canonicalize resolves symlinks and normalises the path without blocking
    // for paths that exist on local storage; for paths that don't exist it errors
    // and we fall back to the raw path.
    if let Ok(c) = fs::canonicalize(path) {
        return Ok(c);
    }
    if path.is_absolute() {
        return Ok(path.clone());
    }
    Ok(env::current_dir()?.join(path))
}

pub(crate) fn register_recent_project(config: &mut AppConfig, path: &PathBuf, name: &str) {
    let canonical_path = canonical_project_path(path).unwrap_or(path.clone());
    let path_string = canonical_path.to_string_lossy().to_string();
    config
        .recent_projects
        .retain(|current| current.project_path != path_string);
    config.recent_projects.insert(
        0,
        RecentProjectEntry {
            project_path: path_string,
            project_name: if name.trim().is_empty() {
                UNTITLED_PROJECT_NAME.to_string()
            } else {
                name.trim().to_string()
            },
            is_valid: true,
            invalid_reason: None,
        },
    );
}

pub(crate) fn mark_recent_project_invalid(config: &mut AppConfig, path: &PathBuf, reason: &str) {
    let canonical_path = canonical_project_path(path).unwrap_or(path.clone());
    let path_string = canonical_path.to_string_lossy().to_string();
    if let Some(recent) = config
        .recent_projects
        .iter_mut()
        .find(|current| current.project_path == path_string)
    {
        recent.is_valid = false;
        recent.invalid_reason = Some(if reason.trim().is_empty() {
            "Projeto inválido".to_string()
        } else {
            reason.trim().to_string()
        });
    }
}

pub(crate) fn recent_project_items(
    recent_projects: &[RecentProjectEntry],
    query: &str,
) -> Vec<RecentProjectItem> {
    let query = query.trim().to_lowercase();
    recent_projects
        .iter()
        .enumerate()
        .filter(|(_, recent)| {
            if query.is_empty() {
                return true;
            }
            recent.project_name.to_lowercase().contains(&query)
                || recent.project_path.to_lowercase().contains(&query)
        })
        .map(|(original_index, recent)| RecentProjectItem {
            original_index: original_index as i32,
            title: if recent.project_name.trim().is_empty() {
                UNTITLED_PROJECT_NAME.into()
            } else {
                recent.project_name.clone().into()
            },
            subtitle: recent.project_path.clone().into(),
            is_valid: recent.is_valid,
            invalid_reason: recent.invalid_reason.clone().unwrap_or_default().into(),
        })
        .collect()
}

pub(crate) fn project_display_name(project: &Project) -> String {
    project
        .name
        .as_ref()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| UNTITLED_PROJECT_NAME.to_string())
}

pub(crate) fn parse_path_argument(flag: &str) -> Option<PathBuf> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next().map(PathBuf::from);
        }
    }
    None
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
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
    };
    ProjectSession::new(
        project,
        None,
        None,
        config.presets_path.unwrap_or_else(default_presets_path),
    )
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
    let rig = std::rc::Rc::new(std::cell::RefCell::new(rig));
    // #436: the dispatcher owns the rig so rig-nav goes through Command
    // (GUI/MIDI/MCP share one path). Same Rc the GUI renders from.
    session.dispatcher.attach_rig(std::rc::Rc::clone(&rig));
    session.rig = Some(rig);
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
            match save_project_session(session, path) {
                Ok(()) => {
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

pub(crate) fn save_project_session(session: &ProjectSession, project_path: &PathBuf) -> Result<()> {
    log::info!("saving project session to {:?}", project_path);
    let parent_dir = project_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent_dir)?;
    // #449/#450: the `RigProject` is the only source of truth that
    // round-trips on reload (`load_project_any` is idempotent and
    // prefers the existing `.openrig` over any legacy `.yaml`). So
    // every save must produce a `.openrig` that reflects what the user
    // currently sees in the chains screen — even when the in-memory
    // session has no rig attached (brand-new project) and even when
    // the user added or removed chains since the last reload.
    let openrig = if project_path.extension().and_then(|e| e.to_str()) == Some("openrig") {
        project_path.clone()
    } else {
        project_path.with_extension("openrig")
    };
    let rig_to_save = build_rig_for_save(session);
    infra_yaml::save_rig_project_file(&openrig, &rig_to_save)?;
    // Also write the legacy `.yaml` snapshot when the user-facing path
    // points at one (the recents list, file dialogs, and CLI args all
    // historically use `.yaml`). The `.openrig` is the canonical source
    // of truth on reload — `load_project_any` finds it via the sibling
    // and ignores the `.yaml` body — but keeping the `.yaml` on disk
    // means an old shortcut/recent entry still resolves to a file.
    if openrig != *project_path {
        fs::write(project_path, project_session_snapshot(session)?)?;
    }
    let config_path = session
        .config_path
        .clone()
        .unwrap_or_else(|| resolve_project_config_path(project_path));
    let config = ConfigYaml {
        presets_path: "./presets".to_string(),
    };
    fs::write(config_path, serde_yaml::to_string(&config)?)?;
    fs::create_dir_all(parent_dir.join("presets"))?;
    Ok(())
}

/// Build the `RigProject` that should land on disk for this session.
///
/// Three sources of truth are reconciled:
///
/// 1. **The attached rig** (if any), which holds scenes, preset names,
///    `active_preset`/`active_scene`, and any rig-level edits made via
///    `Command::ApplyRigNav` / `RenameRigPreset`. We start from a clone
///    of it so those are preserved verbatim.
/// 2. **Edits to projected chains** (ids prefixed `rig:`): a
///    `Command::CaptureRigEdits` dispatch copies the projected chain
///    blocks back into the rig before we clone.
/// 3. **Brand-new chains** in the legacy `Project` whose id does *not*
///    start with `rig:` (e.g. created via `Command::AddChain` in a
///    new-project session). These are migrated through
///    [`migrate_legacy_project`] and merged into the rig.
///
/// Conversely, projected chains that no longer appear in
/// `session.project.chains` (the user deleted them in the UI) are
/// dropped from the rig so the next reload doesn't resurrect them.
fn build_rig_for_save(session: &ProjectSession) -> project::rig::RigProject {
    use std::collections::BTreeSet;
    if session.rig.is_some() {
        let _ = session.dispatcher.dispatch(Command::CaptureRigEdits);
    }
    let mut rig_out = match &session.rig {
        Some(rig) => rig.borrow().clone(),
        None => project::rig::RigProject {
            name: session.project.borrow().name.clone(),
            inputs: std::collections::BTreeMap::new(),
            outputs: std::collections::BTreeMap::new(),
            presets: std::collections::BTreeMap::new(),
            midi: None,
            chain_order: Vec::new(),
        },
    };
    // 1. New chains (not projected from the rig) → each becomes its
    //    own input + preset bank. Migrating per-chain (rather than the
    //    whole legacy project at once) avoids `migrate_legacy_project`'s
    //    auto-grouping by capture source: two chains that happen to
    //    share a device must remain two independent inputs because the
    //    user explicitly created two chains.
    let new_chains: Vec<Chain> = session
        .project
        .borrow()
        .chains
        .iter()
        .filter(|c| !c.id.0.starts_with("rig:"))
        .cloned()
        .collect();
    let mut newly_added_inputs: BTreeSet<String> = BTreeSet::new();
    for chain in new_chains {
        let temp = Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![chain],
        };
        let mut migrated = project::migrate::migrate_legacy_project(&temp);
        // Single-chain migration ⇒ exactly one input ("input-1"). Pop
        // it, retarget the bank entry to a unique preset key, set the
        // visible "Preset 1" default, and ensure scene 1 exists.
        let (_old_input_name, mut input) = migrated
            .inputs
            .iter()
            .next()
            .map(|(k, v)| (k.clone(), v.clone()))
            .expect("single-chain migration produces exactly one input");
        // Generate the next unique input slot in `rig_out`.
        let next_n = rig_out
            .inputs
            .keys()
            .chain(newly_added_inputs.iter())
            .filter_map(|k| k.strip_prefix("input-").and_then(|n| n.parse::<usize>().ok()))
            .max()
            .unwrap_or(0)
            + 1;
        let new_input_name = format!("input-{next_n}");
        // The migrated bank's slot 1 names a preset (slug of the chain
        // description). Two chains can slug to the same key, so we
        // ensure uniqueness in `rig_out.presets`.
        let old_preset_key = input
            .bank
            .get(&1)
            .cloned()
            .expect("migrated input bank slot 1 exists");
        let mut preset = migrated
            .presets
            .remove(&old_preset_key)
            .expect("preset for bank slot 1 exists");
        let mut final_preset_key = old_preset_key.clone();
        let mut suffix = 2;
        while rig_out.presets.contains_key(&final_preset_key) {
            final_preset_key = format!("{old_preset_key}-{suffix}");
            suffix += 1;
        }
        if final_preset_key != old_preset_key {
            input.bank.insert(1, final_preset_key.clone());
        }
        preset.id = final_preset_key.clone();
        // Distinct, user-facing preset label so the chain name and the
        // preset name don't collide ("Chain 1" vs "Preset 1").
        preset.name = Some("Preset 1".to_string());
        // Make scene 1 an addressable slot so the user can edit it
        // without a "create scene" step.
        preset
            .scenes
            .entry(1)
            .or_insert_with(project::rig::RigScene::default);

        rig_out.inputs.insert(new_input_name.clone(), input);
        rig_out.presets.insert(final_preset_key, preset);
        newly_added_inputs.insert(new_input_name);

        for (name, output) in migrated.outputs {
            rig_out.outputs.entry(name).or_insert(output);
        }
    }
    // 2. Projected chains the user removed → drop the matching inputs
    //    so the reload doesn't resurrect them.
    let surviving_projected: BTreeSet<String> = session
        .project
        .borrow()
        .chains
        .iter()
        .filter_map(|c| c.id.0.strip_prefix("rig:").map(String::from))
        .collect();
    rig_out.inputs.retain(|name, _| {
        surviving_projected.contains(name) || newly_added_inputs.contains(name)
    });
    // Garbage-collect orphan presets / outputs no longer referenced.
    let referenced_presets: BTreeSet<String> = rig_out
        .inputs
        .values()
        .flat_map(|i| i.bank.values().cloned())
        .collect();
    rig_out
        .presets
        .retain(|name, _| referenced_presets.contains(name));
    let referenced_outputs: BTreeSet<String> = rig_out
        .inputs
        .values()
        .flat_map(|i| i.routing.iter().cloned())
        .collect();
    rig_out
        .outputs
        .retain(|name, _| referenced_outputs.contains(name));
    // Carry the user-visible project name (`UpdateProjectName` writes
    // to the legacy `Project`; the rig must mirror it on disk).
    rig_out.name = session.project.borrow().name.clone();
    rig_out
}

pub(crate) fn save_chain_blocks_to_preset(chain: &Chain, path: &Path) -> Result<()> {
    let effect_blocks = chain
        .blocks
        .iter()
        .filter(|b| !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
        .cloned()
        .collect();
    let preset = ChainBlocksPreset {
        id: preset_id_from_path(path)?,
        name: chain.description.clone(),
        volume: chain.volume,
        blocks: effect_blocks,
    };
    save_chain_preset_file(path, &preset)
}

pub(crate) fn load_preset_file(path: &Path) -> Result<ChainBlocksPreset> {
    load_chain_preset_file(path)
}

pub(crate) fn preset_id_from_path(path: &Path) -> Result<String> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow!("{}", rust_i18n::t!("error-invalid-preset-file")))
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
#[path = "project_admin_persistence_tests.rs"]
mod project_admin_persistence_tests;


#[cfg(test)]
#[path = "project_rig_persistence_tests.rs"]
mod project_rig_persistence_tests;


#[cfg(test)]
#[path = "project_chain_defaults_persistence_tests.rs"]
mod project_chain_defaults_persistence_tests;
