//! Issue #607 — a path override picked in Settings → Caminhos must survive a
//! subsequent **whole-config** re-save of the in-memory `AppConfig` snapshot
//! (which happens on every project-open / register-recent).
//!
//! User reproduction:
//!   1. Settings → CAMINHOS → ESCOLHER... for "Pasta de avaliações".
//!   2. Open / switch project (any lifecycle event that re-persists the
//!      in-memory app config via `save_app_config(&app_config.borrow())`).
//!   3. `config.yaml` `paths.evaluations_path` is back to `null` — the pick
//!      is lost. `presets_path` / `plugins_path` only appear to survive
//!      because they were loaded into the in-memory snapshot at a prior
//!      startup; one set in the current session would be clobbered the same.
//!
//! Root cause: the Settings pickers persist straight to disk
//! (`save_*_path`) but never update the shared in-memory `AppConfig`. The
//! next whole-config save writes the stale snapshot, reverting the override.
//!
//! Fix contract: applying an override must update the in-memory `AppConfig`
//! in lockstep with the disk write, so a later whole-config save carries the
//! user's pick. This test exercises that seam (`apply_evaluations_override`)
//! and is RED today (the function does not exist).

use std::path::PathBuf;
use std::sync::Mutex;

use adapter_gui::{apply_evaluations_override, apply_plugins_override, apply_presets_override};
use infra_filesystem::FilesystemStorage;

/// HOME is process-global; serialize tests that swap it.
static HOME_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home<F: FnOnce(&PathBuf)>(label: &str, f: F) {
    let _g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp =
        std::env::temp_dir().join(format!("openrig-607-{label}-{}-{now}", std::process::id()));
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
fn issue_607_evaluations_override_survives_whole_config_resave() {
    with_temp_home("eval-clobber", |_| {
        // App startup: the in-memory snapshot is loaded from config.yaml.
        // Fresh HOME → no override yet.
        let mut in_memory = FilesystemStorage::load_app_config().expect("initial load");
        assert_eq!(
            in_memory.paths.evaluations_path, None,
            "fresh config must start with no evaluations override"
        );

        // User picks the evaluations folder. The picker must both persist to
        // disk AND keep the in-memory snapshot in sync.
        let picked = PathBuf::from("/tmp/openrig-607-picked-evaluations");
        apply_evaluations_override(&mut in_memory, Some(picked.clone()))
            .expect("apply_evaluations_override must persist");

        // The in-memory snapshot now carries the pick (so a later whole-config
        // save cannot clobber it).
        assert_eq!(
            in_memory.paths.evaluations_path,
            Some(picked.clone()),
            "the in-memory AppConfig must reflect the picked override"
        );

        // Lifecycle event (open project / register recent) re-persists the
        // whole in-memory snapshot.
        FilesystemStorage::save_app_config(&in_memory).expect("whole-config re-save");

        // The pick must still be there after the round-trip.
        let reloaded = FilesystemStorage::load_app_config().expect("reload");
        assert_eq!(
            reloaded.paths.evaluations_path,
            Some(picked),
            "REGRESSION #607: a whole-config re-save clobbered the user's \
             evaluations pick back to null"
        );
    });
}

#[test]
fn issue_607_all_three_overrides_survive_whole_config_resave() {
    with_temp_home("all-three", |_| {
        let mut in_memory = FilesystemStorage::load_app_config().expect("initial load");

        let presets = PathBuf::from("/tmp/openrig-607-presets");
        let plugins = PathBuf::from("/tmp/openrig-607-plugins");
        let evals = PathBuf::from("/tmp/openrig-607-evals");

        apply_presets_override(&mut in_memory, Some(presets.clone())).expect("presets");
        apply_plugins_override(&mut in_memory, Some(plugins.clone())).expect("plugins");
        apply_evaluations_override(&mut in_memory, Some(evals.clone())).expect("evals");

        // Whole-config re-save of the (now-synced) snapshot must keep all three.
        FilesystemStorage::save_app_config(&in_memory).expect("whole-config re-save");

        let reloaded = FilesystemStorage::load_app_config().expect("reload");
        assert_eq!(
            reloaded.paths.presets_path,
            Some(presets),
            "presets clobbered"
        );
        assert_eq!(
            reloaded.paths.plugins_path,
            Some(plugins),
            "plugins clobbered"
        );
        assert_eq!(
            reloaded.paths.evaluations_path,
            Some(evals),
            "evaluations clobbered"
        );
    });
}
