//! The system `config.yaml` sits at the root of the user folder. A project
//! saved in that same folder writes its sidecar `config.yaml` beside itself —
//! the very same file. Saving must never replace the system config with the
//! project's one-line sidecar, or every device, binding and setting is lost.
#![cfg(unix)]

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, ProjectCommand};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use project::project::Project;

#[test]
fn saving_a_project_in_the_user_folder_keeps_the_system_config() {
    let home = tempfile::tempdir().expect("home");
    let prev = std::env::var_os("HOME");
    std::env::set_var("HOME", home.path());

    let root = infra_filesystem::user_data_root();
    std::fs::create_dir_all(&root).expect("user folder");
    let system_config = infra_filesystem::FilesystemStorage::app_config_path().expect("path");
    let original = "language: pt-BR\nplugins_path: /opt/plugins\n";
    std::fs::write(&system_config, original).expect("seed system config");

    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })));
    dispatcher.attach_project_path(root.join("project.yaml"));
    dispatcher
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .expect("save");
    application::persist_worker::flush();

    let after = std::fs::read_to_string(&system_config).expect("system config still there");
    match prev {
        Some(p) => std::env::set_var("HOME", p),
        None => std::env::remove_var("HOME"),
    }
    assert_eq!(
        after, original,
        "the project sidecar replaced the system config"
    );
    assert!(
        root.join("project.yaml").exists(),
        "the project itself is saved"
    );
}
