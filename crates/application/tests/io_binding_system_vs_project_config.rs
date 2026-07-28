//! #792 / ADR-0003: the per-machine I/O binding registry must persist to the
//! SYSTEM config, never to the project sidecar — even when a project is open
//! (which sets the dispatcher's `config_path` to `<project>/config.yaml`).
//!
//! Before the fix, `handle_create_or_update_io_binding` resolved its target
//! from `config_path` (the project sidecar), so with a project open it wrote
//! the whole `AppConfig` (registry + recents + flags) into the project sidecar
//! and the per-machine system config never received the registry.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, IoBindingCommand};
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

fn make_binding(id: &str) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: id.to_string(),
        inputs: vec![IoEndpoint {
            name: "Guitar In".to_string(),
            device_id: DeviceId("hw:0,0".to_string()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![],
    }
}

#[test]
fn io_binding_registry_lands_in_system_config_not_project_sidecar() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    // Two DISTINCT files: the project sidecar (what a live project attaches to
    // `config_path`) and the per-machine system config.
    let sidecar = tmp.path().join("project_sidecar_config.yaml");
    let system = tmp.path().join("system_config.yaml");

    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_config_path(Some(sidecar.clone())); // project is "open"
    dispatcher.attach_io_config_path(Some(system.clone())); // per-machine config

    dispatcher
        .dispatch(Command::IoBinding(IoBindingCommand::CreateIoBinding {
            binding: make_binding("main"),
        }))
        .expect("CreateIoBinding dispatch ok");
    application::persist_worker::flush();

    // The per-machine registry must be persisted to the SYSTEM config...
    let system_yaml =
        std::fs::read_to_string(&system).expect("system config must be written with the registry");
    assert!(
        system_yaml.contains("main"),
        "the I/O binding registry must land in the system config, got:\n{system_yaml}"
    );

    // ...and NEVER leak into the project sidecar.
    if let Ok(sidecar_yaml) = std::fs::read_to_string(&sidecar) {
        assert!(
            !sidecar_yaml.contains("io_bindings"),
            "the per-machine registry must not leak into the project sidecar, got:\n{sidecar_yaml}"
        );
    }
}
