//! Issue #582 — `Command::SetEvaluationsPath` must persist the picked
//! folder to `config.yaml` exactly like `SetPluginsPath` / `SetPresetsPath`
//! (issue #540).
//!
//! This test exercises the full intended contract: dispatch the command,
//! then load `config.yaml` from the same location the app writes/reads at
//! runtime, and assert the path is present.
//!
//! Red-first today: `Command::SetEvaluationsPath` does not exist yet.
//! Adding it + wiring its handler to persist (mirroring the existing two
//! path commands) is what turns this test green.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Mutex;

use application::command::Command;
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

fn with_temp_home<F: FnOnce(&PathBuf)>(label: &str, f: F) {
    let _g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp =
        std::env::temp_dir().join(format!("openrig-582-{label}-{}-{now}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mkdir tempdir");
    let prev = std::env::var_os("HOME");
    std::env::set_var("HOME", &tmp);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&tmp)));
    if let Some(prev) = prev {
        std::env::set_var("HOME", prev);
    } else {
        std::env::remove_var("HOME");
    }
    let _ = std::fs::remove_dir_all(&tmp);
    if let Err(p) = res {
        std::panic::resume_unwind(p);
    }
}

#[test]
fn issue_582_set_evaluations_path_persists_to_config_yaml() {
    with_temp_home("eval-override", |_| {
        let project = empty_project_rc();
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));
        let picked = PathBuf::from("/tmp/openrig-582-picked-evaluations");

        let events = dispatcher
            .dispatch(Command::SetEvaluationsPath {
                path: Some(picked.clone()),
            })
            .expect("dispatch must succeed");

        assert!(
            !events.is_empty(),
            "dispatch must emit at least one event (mirrors SetPresetsPath / SetPluginsPath)"
        );

        let loaded = FilesystemStorage::load_app_config()
            .expect("load_app_config from fresh HOME must succeed");
        assert_eq!(
            loaded.paths.evaluations_path,
            Some(picked),
            "REGRESSION: Command::SetEvaluationsPath did not persist into config.yaml — \
             user pick survives in memory only and is lost on restart"
        );
    });
}

#[test]
fn issue_582_reset_evaluations_path_persists_none_in_config_yaml() {
    with_temp_home("eval-reset", |_| {
        let project = empty_project_rc();
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        // First: set an override so reset has something to undo.
        let picked = PathBuf::from("/tmp/openrig-582-pre-reset");
        dispatcher
            .dispatch(Command::SetEvaluationsPath {
                path: Some(picked.clone()),
            })
            .expect("set must succeed");

        // Then: reset → None.
        dispatcher
            .dispatch(Command::SetEvaluationsPath { path: None })
            .expect("reset must succeed");

        let loaded = FilesystemStorage::load_app_config()
            .expect("load_app_config from fresh HOME must succeed");
        assert!(
            loaded.paths.evaluations_path.is_none(),
            "REGRESSION: resetting evaluations_path to None must clear it from \
             config.yaml, got: {:?}",
            loaded.paths.evaluations_path
        );
    });
}
