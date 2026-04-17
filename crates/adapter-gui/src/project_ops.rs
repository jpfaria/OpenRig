use anyhow::{anyhow, Result};
use infra_filesystem::{
    AppConfig, FilesystemStorage, GuiAudioDeviceSettings, RecentProjectEntry,
};
use infra_yaml::{load_chain_preset_file, save_chain_preset_file, ChainBlocksPreset, YamlProjectRepository};
use project::block::AudioBlockKind;
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;
use domain::ids::DeviceId;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use crate::state::{ProjectSession, ProjectPaths, AppConfigYaml, ConfigYaml};
use crate::{AppWindow, UNTITLED_PROJECT_NAME};
use crate::RecentProjectItem;

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
            env::current_dir().map(|d| d.join(&path)).unwrap_or(path.clone())
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

pub(crate) fn create_new_project_session(default_config_path: &Path) -> ProjectSession {
    let config = if default_config_path.exists() {
        load_app_config(default_config_path).unwrap_or_default()
    } else {
        AppConfigYaml {
            presets_path: Some(PathBuf::from("./presets")),
        }
    };
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
    };
    ProjectSession {
        project,
        project_path: None,
        config_path: None,
        presets_path: config
            .presets_path
            .unwrap_or_else(|| PathBuf::from("./presets")),
    }
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
            });
        }
    }
    result
}

pub(crate) fn load_project_session(project_path: &Path, config_path: &Path) -> Result<ProjectSession> {
    log::info!("loading project session from {:?}", project_path);
    let config = if config_path.exists() {
        load_app_config(config_path)?
    } else {
        AppConfigYaml::default()
    };
    let presets_path = config
        .presets_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("./presets"));
    let mut project = YamlProjectRepository {
        path: project_path.to_path_buf(),
    }
    .load_current_project()?;

    // Populate device_settings from per-machine config (gui-settings.yaml)
    // instead of the project YAML. Old projects may still have device_settings
    // in their YAML — those are read for backward compat but overridden here.
    let gui_settings = FilesystemStorage::load_gui_audio_settings()
        .ok()
        .flatten()
        .unwrap_or_default();
    project.device_settings = build_device_settings_from_gui(
        &gui_settings.input_devices,
        &gui_settings.output_devices,
    );

    Ok(ProjectSession {
        project,
        project_path: Some(project_path.to_path_buf()),
        config_path: Some(config_path.to_path_buf()),
        presets_path: project_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(presets_path),
    })
}

pub(crate) fn project_session_snapshot(session: &ProjectSession) -> Result<String> {
    infra_yaml::serialize_project(&session.project)
}

pub(crate) fn set_project_dirty(window: &AppWindow, project_dirty: &std::rc::Rc<std::cell::RefCell<bool>>, dirty: bool) {
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
    // Snapshot audio hardware + JACK state for every UI-triggered mutation so we
    // can correlate a specific user action (knob, device pick, chain edit) with
    // downstream Scarlett/xHCI disconnects in the journal. The caller location
    // identifies which UI callback fired this mutation.
    let caller = std::panic::Location::caller();
    infra_cpal::log_audio_status(&format!(
        "sync_project_dirty from {}:{}",
        caller.file(),
        caller.line()
    ));

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
    fs::write(project_path, project_session_snapshot(session)?)?;
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
        .ok_or_else(|| anyhow!("arquivo de preset inválido"))
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
