//! #980: an I/O binding edit must survive the recent-projects write that
//! follows every project open, open-recent, save and remove-recent.
//!
//! Owner's reproduction: `syn2-main` output set to `[3]` over MCP
//! (`UpdateIoBinding` → the dispatcher's registry + a read-modify-write of
//! `config.yaml`). The GUI's own `AppConfig`, loaded at boot, still held
//! `[7]`. The next project open wrote that WHOLE boot-time snapshot back to
//! `config.yaml` just to record a recent project, and `[7]` returned — on
//! 23/09 and twice on 24/09, each time fixed by hand and each time back.
//!
//! Contract: the recent-projects write changes `recent_projects` and nothing
//! else. Every write targets a `tempfile` directory (#701 / #731).

use std::path::Path;

use application::app_config_persist::persist_recent_projects;
use domain::ids::DeviceId;
use infra_filesystem::{
    AppConfig, ChannelMode, FilesystemStorage, IoBinding, IoEndpoint, RecentProjectEntry,
};

const DEVICE: &str = "coreaudio:TUSBAudio:Fender:Quantum HD 8";

fn syn2_main(send_channel: usize) -> IoBinding {
    IoBinding {
        id: "syn2-main".to_string(),
        name: "PEDAIS + SYN-2".to_string(),
        inputs: vec![IoEndpoint {
            name: "SYN-2 DI OUT L/R".to_string(),
            device_id: DeviceId(DEVICE.to_string()),
            mode: ChannelMode::Stereo,
            channels: vec![2, 3],
        }],
        outputs: vec![IoEndpoint {
            name: "pedais".to_string(),
            device_id: DeviceId(DEVICE.to_string()),
            mode: ChannelMode::Mono,
            channels: vec![send_channel],
        }],
    }
}

fn send_channels(config: &Path) -> Vec<usize> {
    let on_disk = FilesystemStorage::load_app_config_at(config).expect("reload config");
    on_disk
        .io_bindings
        .iter()
        .find(|b| b.id == "syn2-main")
        .expect("syn2-main is still registered")
        .outputs[0]
        .channels
        .clone()
}

#[test]
fn recording_a_recent_project_keeps_an_io_binding_edited_after_boot() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    // Boot: the GUI loads its AppConfig while the send is on [7].
    FilesystemStorage::save_io_bindings_at(&config, vec![syn2_main(7)]).expect("seed");
    let mut boot_snapshot = FilesystemStorage::load_app_config_at(&config).expect("boot load");

    // Later, over MCP: the binding is fixed to [3] on disk.
    FilesystemStorage::save_io_bindings_at(&config, vec![syn2_main(3)]).expect("edit");

    // Then the owner opens a project: the GUI records it as recent in its
    // boot-time snapshot and persists.
    boot_snapshot.recent_projects.insert(
        0,
        RecentProjectEntry {
            project_path: "/tmp/project.yaml".to_string(),
            project_name: "project".to_string(),
            is_valid: true,
            invalid_reason: None,
        },
    );
    persist_recent_projects(Some(config.clone()), &boot_snapshot);
    application::persist_worker::flush();

    assert_eq!(
        send_channels(&config),
        vec![3],
        "recording a recent project wrote the boot-time I/O bindings back over the edit"
    );
    let on_disk: AppConfig = FilesystemStorage::load_app_config_at(&config).expect("reload");
    assert_eq!(
        on_disk.recent_projects, boot_snapshot.recent_projects,
        "the recent-projects list itself must still be persisted"
    );
}
