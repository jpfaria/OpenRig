//! Responsibility: opens a project into a session.

use crate::app_config_load::{
    default_presets_path, ensure_default_io_binding, load_app_config, resolve_project_config_path,
};
use crate::gui_device_settings::build_device_settings_from_gui;
use crate::state::{AppConfigYaml, ProjectSession};
use anyhow::Result;
use application::command::{Command, SelectionCommand};
use infra_filesystem::FilesystemStorage;
use infra_yaml::{load_chain_preset_file, ChainBlocksPreset};
use project::project::Project;
use std::path::{Path, PathBuf};
// Issue #792 split: recent-projects + path/name helpers live in
// project_ops_recents.rs. Re-exported so crate::project_ops::* and the
// super:: paths in the persistence test modules keep resolving.

pub(crate) fn open_cli_project(path: &PathBuf) -> Result<ProjectSession> {
    if !path.exists() {
        anyhow::bail!("CLI project path does not exist: {:?}", path);
    }
    let config_path = resolve_project_config_path(path);
    load_project_session(path, &config_path)
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
    // Attach an empty rig from the start so `ChainCommand::AddChain` can
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

    // #716: clean break from the old project format. Routing is binding-only —
    // the per-machine io_bindings registry (config.yaml) is the single source
    // of truth for I/O. There is NO legacy-entries migration: a legacy project
    // (Input/Output blocks with `entries` but empty `io`) opens UNBOUND and
    // must be reconfigured via the registry. Hand the existing registry to the
    // session so the live runtime resolves bound chains per binding.
    let registry_bindings: Vec<infra_filesystem::IoBinding> = FilesystemStorage::load_app_config()
        .map(|cfg| cfg.io_bindings)
        .unwrap_or_default();

    // #606 + #833: load-time passes shared with the dispatcher's LoadProject.
    crate::project_load_normalize::normalize_loaded_project(&mut project, &registry_bindings);

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
        let _ =
            session
                .dispatcher
                .dispatch(Command::Selection(SelectionCommand::SelectActiveChain {
                    chain: first_chain,
                }));
    }

    Ok(session)
}

pub(crate) fn load_preset_file(path: &Path) -> Result<ChainBlocksPreset> {
    load_chain_preset_file(path)
}

// `preset_id_from_path` lives inside `local_dispatcher_preset` now —
// the file id is derived at write time from the path the dispatcher
// resolves, not from a GUI helper.
