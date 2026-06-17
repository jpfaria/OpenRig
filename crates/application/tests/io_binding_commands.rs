//! Task 3 — CreateIoBinding / UpdateIoBinding / DeleteIoBinding commands.
//!
//! Contract:
//! - CreateIoBinding  → binding stored in in-memory AppConfig snapshot AND
//!   persisted to config.yaml; reload reads it back.
//! - UpdateIoBinding  → upsert by id: fields change, count unchanged.
//! - DeleteIoBinding  → binding removed (no reference-check enforcement yet;
//!   that guard is deferred to Task 5 when chain blocks exist).
//!
//! ## Test isolation
//!
//! The handlers call `FilesystemStorage::load/save_app_config()`, which
//! resolves the config path via `dirs::config_dir()` (system API on macOS)
//! with a HOME-based fallback. Because `dirs::config_dir()` on macOS uses
//! `NSFileManager` rather than the HOME env var, the HOME-swap here is
//! effective only on Linux/Windows (HOME-derived `.config`).
//!
//! On macOS the test therefore writes to the user's real
//! `~/Library/Application Support/OpenRig/config.yaml` — a pre-existing
//! issue shared with the `issue_693` test suite. A proper fix requires the
//! handlers to accept an injectable path (separate follow-up).
//!
//! Until that follow-up lands the three tests below **must run serially** to
//! avoid racing on the shared config path. `ENV_LOCK` provides that
//! serialisation without adding the `serial_test` crate.
//!
//! ### Why not serial_test?
//! The repo has no `serial_test` dependency and the rules prohibit adding
//! crates speculatively. A plain `std::sync::Mutex` achieves the same
//! guarantee with zero new dependencies.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::project::Project;

/// Serialises all three tests so they cannot race on the global HOME / config
/// path. Held for the full test body — acquire at the top, drop at the end.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn empty_project() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    }))
}

fn make_binding(id: &str, name: &str) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: name.to_string(),
        inputs: vec![IoEndpoint {
            name: "Guitar In".to_string(),
            device_id: DeviceId("hw:0,0".to_string()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![],
    }
}

// ---------------------------------------------------------------------------
// test_create_then_persists
// ---------------------------------------------------------------------------

#[test]
fn test_create_then_persists() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");
    let tmp = tempfile::TempDir::new().expect("tempdir");
    // HOME fallback: effective on Linux/Windows; macOS uses NSFileManager.
    // SAFETY: held under ENV_LOCK — no concurrent HOME mutation in this suite.
    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var("HOME", tmp.path());
    }

    let dispatcher = LocalDispatcher::new(empty_project());

    let binding = make_binding("main", "Main");
    dispatcher
        .dispatch(Command::CreateIoBinding {
            binding: binding.clone(),
        })
        .expect("CreateIoBinding dispatch ok");

    // Flush the persist worker so the disk write completes before we read.
    application::persist_worker::flush();

    // Verify: config.yaml must exist and contain the binding.
    let cfg_path = infra_filesystem::FilesystemStorage::app_config_path()
        .expect("config path resolvable");
    assert!(
        cfg_path.exists(),
        "config.yaml must exist after CreateIoBinding — persistence not wired"
    );

    let raw = std::fs::read_to_string(&cfg_path).expect("read config.yaml");
    assert!(
        raw.contains("main"),
        "config.yaml must contain binding id 'main' after CreateIoBinding; got:\n{raw}"
    );
    assert!(
        raw.contains("Main"),
        "config.yaml must contain binding name 'Main' after CreateIoBinding; got:\n{raw}"
    );
}

// ---------------------------------------------------------------------------
// test_update_replaces_by_id
// ---------------------------------------------------------------------------

#[test]
fn test_update_replaces_by_id() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");
    let tmp = tempfile::TempDir::new().expect("tempdir");
    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var("HOME", tmp.path());
    }

    let dispatcher = LocalDispatcher::new(empty_project());

    let original = make_binding("rig1", "Rig 1");
    dispatcher
        .dispatch(Command::CreateIoBinding {
            binding: original.clone(),
        })
        .expect("create ok");

    // Also create a second binding so we can assert count is unchanged.
    let other = make_binding("rig2", "Rig 2");
    dispatcher
        .dispatch(Command::CreateIoBinding {
            binding: other.clone(),
        })
        .expect("create second ok");

    // Update rig1 with a new name.
    let updated = IoBinding {
        id: "rig1".to_string(),
        name: "Rig 1 Updated".to_string(),
        inputs: vec![],
        outputs: vec![IoEndpoint {
            name: "Monitor Out".to_string(),
            device_id: DeviceId("hw:0,1".to_string()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    };
    dispatcher
        .dispatch(Command::UpdateIoBinding {
            binding: updated.clone(),
        })
        .expect("update ok");

    application::persist_worker::flush();

    // Re-parse the persisted config rather than doing YAML substring matching.
    // This is format-independent and won't break if serde changes quoting.
    let cfg_path = infra_filesystem::FilesystemStorage::app_config_path()
        .expect("config path");
    let raw = std::fs::read_to_string(&cfg_path).expect("read config.yaml");
    let persisted: infra_filesystem::AppConfig =
        serde_yaml::from_str(&raw).expect("config.yaml must be valid YAML after update");

    assert_eq!(
        persisted.io_bindings.len(),
        2,
        "binding count must be unchanged after update (upsert, not insert); \
         got {} bindings:\n{raw}",
        persisted.io_bindings.len()
    );

    let updated_b = persisted
        .io_bindings
        .iter()
        .find(|b| b.id == "rig1")
        .expect("binding 'rig1' must still exist after update");
    assert_eq!(
        updated_b.name, "Rig 1 Updated",
        "binding 'rig1' must have the new name after update"
    );

    assert!(
        persisted.io_bindings.iter().all(|b| b.name != "Rig 1"),
        "old name 'Rig 1' must be absent from all bindings after update"
    );

    assert!(
        persisted.io_bindings.iter().any(|b| b.id == "rig2"),
        "second binding 'rig2' must still be present after update of first"
    );
}

// ---------------------------------------------------------------------------
// test_delete_removes
// ---------------------------------------------------------------------------

#[test]
fn test_delete_removes() {
    let _guard = ENV_LOCK.lock().expect("ENV_LOCK poisoned");
    let tmp = tempfile::TempDir::new().expect("tempdir");
    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var("HOME", tmp.path());
    }

    let dispatcher = LocalDispatcher::new(empty_project());

    let b1 = make_binding("del-me", "Delete Me");
    let b2 = make_binding("keep-me", "Keep Me");
    dispatcher
        .dispatch(Command::CreateIoBinding { binding: b1 })
        .expect("create del-me ok");
    dispatcher
        .dispatch(Command::CreateIoBinding { binding: b2 })
        .expect("create keep-me ok");

    dispatcher
        .dispatch(Command::DeleteIoBinding {
            id: "del-me".to_string(),
        })
        .expect("delete ok");

    application::persist_worker::flush();

    let cfg_path = infra_filesystem::FilesystemStorage::app_config_path()
        .expect("config path");
    let raw = std::fs::read_to_string(&cfg_path).expect("read config.yaml");

    assert!(
        !raw.contains("del-me"),
        "deleted binding id must be absent from config.yaml; got:\n{raw}"
    );
    assert!(
        raw.contains("keep-me"),
        "surviving binding must still be present after delete; got:\n{raw}"
    );
}
