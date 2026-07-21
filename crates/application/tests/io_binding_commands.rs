//! Task 3 — CreateIoBinding / UpdateIoBinding / DeleteIoBinding commands.
//!
//! Contract:
//! - CreateIoBinding  → binding stored in config.yaml; reload reads it back.
//! - UpdateIoBinding  → upsert by id: fields change, count unchanged.
//! - DeleteIoBinding  → binding removed (no reference-check enforcement yet;
//!   that guard is deferred to Task 5 when chain blocks exist).
//!
//! ## Test isolation
//!
//! Each test creates a `TempDir` and calls `attach_io_config_path` with a path
//! inside it, so all reads and writes stay inside the temp directory.
//! The real OS config file (`~/Library/Application Support/OpenRig/config.yaml`
//! on macOS, `~/.config/OpenRig/config.yaml` on Linux) is never touched.
//! No `std::env::set_var("HOME", …)` — no global env mutation at all.

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
    let cfg_path = tmp.path().join("config.yaml");

    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_io_config_path(Some(cfg_path.clone()));

    let binding = make_binding("main", "Main");
    dispatcher
        .dispatch(Command::CreateIoBinding {
            binding: binding.clone(),
        })
        .expect("CreateIoBinding dispatch ok");

    // Flush the persist worker so the disk write completes before we read.
    application::persist_worker::flush();

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
    let cfg_path = tmp.path().join("config.yaml");

    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_io_config_path(Some(cfg_path.clone()));

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
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = tmp.path().join("config.yaml");

    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_io_config_path(Some(cfg_path.clone()));

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

// ---------------------------------------------------------------------------
// Intent commands (#716): AddIoEndpoint / RemoveIoEndpoint / RenameIoBinding.
// ALL logic lives in the handler — these tests dispatch the raw intent (no GUI,
// no domain construction by the caller) and assert the persisted state.
// ---------------------------------------------------------------------------

fn reload(cfg_path: &std::path::Path) -> infra_filesystem::AppConfig {
    let raw = std::fs::read_to_string(cfg_path).expect("read config.yaml");
    serde_yaml::from_str(&raw).expect("parse AppConfig")
}

fn empty_binding(id: &str, name: &str) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: name.to_string(),
        inputs: vec![],
        outputs: vec![],
    }
}

#[test]
fn test_add_input_endpoint_builds_and_persists() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = tmp.path().join("config.yaml");
    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_io_config_path(Some(cfg_path.clone()));

    dispatcher
        .dispatch(Command::CreateIoBinding {
            binding: empty_binding("main", "Main"),
        })
        .expect("create ok");
    // The GUI would only forward these raw picker values; the handler builds
    // the IoEndpoint (name, domain types) and appends it.
    dispatcher
        .dispatch(Command::AddIoEndpoint {
            binding_id: "main".into(),
            is_input: true,
            device_id: "devA".into(),
            channels: vec![0, 1],
            mode: ChannelMode::Stereo,
        })
        .expect("add ok");
    application::persist_worker::flush();

    let cfg = reload(&cfg_path);
    let b = cfg
        .io_bindings
        .iter()
        .find(|b| b.id == "main")
        .expect("binding");
    assert_eq!(b.inputs.len(), 1, "handler must append the input endpoint");
    let ep = &b.inputs[0];
    assert_eq!(ep.name, "In 1", "handler assigns the sequential name");
    assert_eq!(ep.device_id, DeviceId("devA".into()));
    assert_eq!(ep.channels, vec![0, 1]);
    assert_eq!(ep.mode, ChannelMode::Stereo);
    assert!(b.outputs.is_empty());
}

#[test]
fn test_remove_io_endpoint() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = tmp.path().join("config.yaml");
    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_io_config_path(Some(cfg_path.clone()));

    dispatcher
        .dispatch(Command::CreateIoBinding {
            binding: empty_binding("main", "Main"),
        })
        .expect("create");
    dispatcher
        .dispatch(Command::AddIoEndpoint {
            binding_id: "main".into(),
            is_input: true,
            device_id: "devA".into(),
            channels: vec![0],
            mode: ChannelMode::Mono,
        })
        .expect("add");
    dispatcher
        .dispatch(Command::RemoveIoEndpoint {
            binding_id: "main".into(),
            is_input: true,
            endpoint_name: "In 1".into(),
        })
        .expect("remove");
    application::persist_worker::flush();

    let cfg = reload(&cfg_path);
    let b = cfg
        .io_bindings
        .iter()
        .find(|b| b.id == "main")
        .expect("binding");
    assert!(b.inputs.is_empty(), "handler must remove the endpoint");
}

#[test]
fn test_rename_io_binding() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = tmp.path().join("config.yaml");
    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_io_config_path(Some(cfg_path.clone()));

    dispatcher
        .dispatch(Command::CreateIoBinding {
            binding: empty_binding("main", "Old"),
        })
        .expect("create");
    dispatcher
        .dispatch(Command::RenameIoBinding {
            id: "main".into(),
            name: "New Name".into(),
        })
        .expect("rename");
    application::persist_worker::flush();

    let cfg = reload(&cfg_path);
    let b = cfg
        .io_bindings
        .iter()
        .find(|b| b.id == "main")
        .expect("binding");
    assert_eq!(b.name, "New Name");
    assert_eq!(cfg.io_bindings.len(), 1, "rename must not change count");
}
