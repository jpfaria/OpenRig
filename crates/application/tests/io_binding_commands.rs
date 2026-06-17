//! Task 3 — CreateIoBinding / UpdateIoBinding / DeleteIoBinding commands.
//!
//! Contract:
//! - CreateIoBinding  → binding stored in in-memory AppConfig snapshot AND
//!   persisted to config.yaml; reload reads it back.
//! - UpdateIoBinding  → upsert by id: fields change, count unchanged.
//! - DeleteIoBinding  → binding removed (no reference-check enforcement yet;
//!   that guard is deferred to Task 5 when chain blocks exist).

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::project::Project;

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
    let tmp = tempfile::TempDir::new().expect("tempdir");
    // Point HOME to tmp so config.yaml lands there, not the real user HOME.
    std::env::set_var("HOME", tmp.path());

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
    let tmp = tempfile::TempDir::new().expect("tempdir");
    std::env::set_var("HOME", tmp.path());

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

    let cfg_path = infra_filesystem::FilesystemStorage::app_config_path()
        .expect("config path");
    let raw = std::fs::read_to_string(&cfg_path).expect("read config.yaml");

    assert!(
        raw.contains("Rig 1 Updated"),
        "updated name must appear in config.yaml; got:\n{raw}"
    );
    // The old name must be gone — check for the exact YAML value "Rig 1" (not
    // as a prefix of "Rig 1 Updated"). YAML serializes strings inline so we
    // look for the standalone value at end-of-line or space.
    assert!(
        !raw.contains("name: Rig 1\n") && !raw.contains("'Rig 1'"),
        "old name 'Rig 1' must be gone after update; got:\n{raw}"
    );
    assert!(
        raw.contains("rig2") || raw.contains("Rig 2"),
        "second binding must still be present after update of first; got:\n{raw}"
    );
}

// ---------------------------------------------------------------------------
// test_delete_removes
// ---------------------------------------------------------------------------

#[test]
fn test_delete_removes() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    std::env::set_var("HOME", tmp.path());

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
