//! Issue #540 — Settings → Caminhos must persist the picked folder
//! to `config.yaml` so the choice survives a restart.
//!
//! User reproduction:
//!   1. Configurar projeto → CAMINHOS → ESCOLHER... pick a folder for
//!      "Pasta de plugins" (or "Pasta de presets").
//!   2. Inspect `~/Library/Application Support/OpenRig/config.yaml`.
//!   3. `paths.plugins_path` (or `paths.presets_path`) is still `null`.
//!
//! The dispatcher's existing tests
//! (`set_plugins_path_emits_paths_saved`,
//!  `set_presets_path_emits_paths_saved`) only check that
//! `Event::PathsSaved` is emitted — they don't observe filesystem
//! state. So they keep passing while the user's pick never reaches
//! disk.
//!
//! This test exercises the full intended contract: dispatch the
//! Command, then load `config.yaml` from the same location the app
//! writes/reads at runtime, and assert the path is present.
//!
//! It is RED today: `SettingsCommand::SetPluginsPath` and
//! `SettingsCommand::SetPresetsPath` are wired to emit the event only; nothing
//! persists. Fix forward by wiring the persistence into the system
//! command's handler (or by adding the adapter-side listener).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Mutex;

use application::command::{Command, SettingsCommand};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use infra_filesystem::FilesystemStorage;
use project::project::Project;

/// HOME is process-global; serialize tests that swap it.
static HOME_LOCK: Mutex<()> = Mutex::new(());

fn empty_project_rc() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        midi: None,
        chains: vec![],
    }))
}

/// Run `f` with HOME redirected at a unique tempdir. Cleans up on
/// success or panic.
fn with_temp_home<F: FnOnce(&PathBuf)>(label: &str, f: F) {
    let _g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp =
        std::env::temp_dir().join(format!("openrig-540-{label}-{}-{now}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mkdir tempdir");
    let prev = std::env::var_os("HOME");
    // dirs::config_dir() honours $XDG_CONFIG_HOME over $HOME/.config on Linux
    // (CI runners set it), so a HOME-only swap leaks to the runner's real
    // config dir. Track XDG alongside HOME so config paths follow the tempdir.
    let prev_xdg = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("HOME", &tmp);
    std::env::set_var("XDG_CONFIG_HOME", tmp.join(".config"));
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&tmp)));
    if let Some(prev) = prev {
        std::env::set_var("HOME", prev);
    } else {
        std::env::remove_var("HOME");
    }
    if let Some(prev_xdg) = prev_xdg {
        std::env::set_var("XDG_CONFIG_HOME", prev_xdg);
    } else {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
    let _ = std::fs::remove_dir_all(&tmp);
    if let Err(p) = res {
        std::panic::resume_unwind(p);
    }
}

#[test]
fn issue_540_set_plugins_path_persists_to_config_yaml() {
    with_temp_home("plugins", |_| {
        let project = empty_project_rc();
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));
        let picked = PathBuf::from("/tmp/openrig-540-picked-plugins");

        let events = dispatcher
            .dispatch(Command::Settings(SettingsCommand::SetPluginsPath {
                path: Some(picked.clone()),
            }))
            .expect("dispatch must succeed");
        // #693: path persistence runs on the persist worker — wait for
        // durability before reading config.yaml back.
        application::persist_worker::flush();

        assert!(!events.is_empty(), "dispatch must emit at least one event");

        let loaded = FilesystemStorage::load_app_config()
            .expect("load_app_config from fresh HOME must succeed");
        assert_eq!(
            loaded.paths.plugins_path,
            Some(picked),
            "REGRESSION: Command::SetPluginsPath did not persist into config.yaml — \
             user pick survives in memory only and is lost on restart"
        );
    });
}

#[test]
fn issue_540_set_presets_path_persists_to_config_yaml() {
    with_temp_home("presets", |_| {
        let project = empty_project_rc();
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));
        let picked = PathBuf::from("/tmp/openrig-540-picked-presets");

        let events = dispatcher
            .dispatch(Command::Settings(SettingsCommand::SetPresetsPath {
                path: Some(picked.clone()),
            }))
            .expect("dispatch must succeed");
        // #693: path persistence runs on the persist worker — wait for
        // durability before reading config.yaml back.
        application::persist_worker::flush();

        assert!(!events.is_empty(), "dispatch must emit at least one event");

        let loaded = FilesystemStorage::load_app_config()
            .expect("load_app_config from fresh HOME must succeed");
        assert_eq!(
            loaded.paths.presets_path,
            Some(picked),
            "REGRESSION: Command::SetPresetsPath did not persist into config.yaml — \
             user pick survives in memory only and is lost on restart"
        );
    });
}
