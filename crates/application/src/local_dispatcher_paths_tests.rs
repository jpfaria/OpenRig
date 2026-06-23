//! Dispatcher tests for `Command::SetPresetsPath` /
//! `Command::SetPluginsPath` (#513, #540).
//!
//! Pulled out of `local_dispatcher_tests.rs` so that file does not grow
//! past its size cap (per `validate.sh`). #540 made the handler write
//! `config.yaml` directly via `FilesystemStorage::save_*_path`, so each
//! test redirects `$HOME` to a unique tempdir to keep the FS write out
//! of the developer's real `~/Library/Application Support/OpenRig/`.
//!
//! Loaded as a sibling test module from `lib.rs` via
//! `#[cfg(test)] #[path = "local_dispatcher_paths_tests.rs"] mod ...`.

use std::path::PathBuf;
use std::rc::Rc;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::local_dispatcher_tests::empty_project_rc;

/// `$HOME` is process-global; serialise tests that swap it so they
/// don't see each other's tempdir.
static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run `f` with `$HOME` pointed at a fresh tempdir. Restores the
/// previous `$HOME` (or removes it) and deletes the tempdir whether
/// `f` panics or returns normally.
///
/// `pub(super)` so the sibling `local_dispatcher_tests` module can
/// reuse it for commands that now hit `config.yaml` too (#581 made
/// `SaveAudioSettings` persist).
pub(super) fn with_tmp_home<F: FnOnce()>(label: &str, f: F) {
    let _g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = std::env::temp_dir().join(format!(
        "openrig-paths-{label}-{}-{now}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmp).expect("mkdir tempdir");
    let prev = std::env::var_os("HOME");
    std::env::set_var("HOME", &tmp);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    // #731: drain the async persist worker BEFORE restoring `$HOME`, so a
    // queued config write can't land on the real config after the swap
    // unwinds. Defense-in-depth alongside dispatch-time path binding.
    crate::persist_worker::flush();
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
fn set_presets_path_emits_paths_saved() {
    with_tmp_home("presets-emit", || {
        let project = empty_project_rc();
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));
        let path = PathBuf::from("/tmp/openrig-test-presets");

        let events = dispatcher
            .dispatch(Command::SetPresetsPath {
                path: Some(path.clone()),
            })
            .unwrap();

        assert_eq!(events, vec![Event::PathsSaved]);
        // System command must not touch the project itself.
        assert!(project.borrow().chains.is_empty());
        assert!(project.borrow().midi.is_none());
    });
}

#[test]
fn set_plugins_path_emits_paths_saved() {
    with_tmp_home("plugins-emit", || {
        let project = empty_project_rc();
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));
        let path = PathBuf::from("/tmp/openrig-test-plugins");

        let events = dispatcher
            .dispatch(Command::SetPluginsPath {
                path: Some(path.clone()),
            })
            .unwrap();

        assert_eq!(events, vec![Event::PathsSaved]);
        assert!(project.borrow().chains.is_empty());
        assert!(project.borrow().midi.is_none());
    });
}

#[test]
fn set_presets_path_none_resets_to_default_and_still_emits() {
    with_tmp_home("presets-none", || {
        let project = empty_project_rc();
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        let events = dispatcher
            .dispatch(Command::SetPresetsPath { path: None })
            .unwrap();
        assert_eq!(events, vec![Event::PathsSaved]);
    });
}

#[test]
fn set_plugins_path_none_resets_to_default_and_still_emits() {
    with_tmp_home("plugins-none", || {
        let project = empty_project_rc();
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        let events = dispatcher
            .dispatch(Command::SetPluginsPath { path: None })
            .unwrap();
        assert_eq!(events, vec![Event::PathsSaved]);
    });
}
