//! #913 — replacing the binding registry without touching the rest of config.
//!
//! `config.yaml` holds far more than bindings (paths, GUI settings, MIDI). The
//! whole reason this function exists instead of a load-mutate-save at every
//! call site is that everything else in the file must survive a registry
//! replacement — a save that rewrote the document from defaults would silently
//! wipe the user's configured paths every time they edited an I/O binding.

use super::{ChannelMode, IoBinding, IoEndpoint};
use crate::FilesystemStorage;
use domain::ids::DeviceId;
use std::path::PathBuf;

fn binding(id: &str) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![IoEndpoint {
            name: "In 1".into(),
            device_id: DeviceId("dev-in".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out 1".into(),
            device_id: DeviceId("dev-out".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

/// A throwaway config file. The directory is deleted when the guard drops, so
/// no test ever writes near the machine's real `config.yaml`.
fn config_file() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    (dir, path)
}

#[test]
fn saving_bindings_into_a_fresh_config_writes_them() {
    let (_guard, path) = config_file();
    FilesystemStorage::save_io_bindings_at(&path, vec![binding("io-main")]).expect("save");
    let back = FilesystemStorage::load_app_config_at(&path).expect("load");
    assert_eq!(back.io_bindings.len(), 1);
    assert_eq!(back.io_bindings[0].id, "io-main");
}

#[test]
fn replacing_the_registry_preserves_every_other_config_field() {
    let (_guard, path) = config_file();

    let mut seeded = crate::AppConfig::default();
    seeded.paths.presets_path = Some(PathBuf::from("/tmp/openrig-presets-913"));
    seeded.io_bindings = vec![binding("io-old")];
    FilesystemStorage::save_app_config_at(&path, &seeded).expect("seed");

    FilesystemStorage::save_io_bindings_at(&path, vec![binding("io-new")]).expect("save");

    let back = FilesystemStorage::load_app_config_at(&path).expect("load");
    assert_eq!(
        back.io_bindings
            .iter()
            .map(|b| b.id.as_str())
            .collect::<Vec<_>>(),
        vec!["io-new"],
        "the registry is REPLACED, not merged"
    );
    assert_eq!(
        back.paths.presets_path,
        Some(PathBuf::from("/tmp/openrig-presets-913")),
        "an unrelated setting must survive a binding edit"
    );
}

#[test]
fn saving_an_empty_registry_clears_it() {
    let (_guard, path) = config_file();
    FilesystemStorage::save_io_bindings_at(&path, vec![binding("io-main")]).expect("save");
    FilesystemStorage::save_io_bindings_at(&path, Vec::new()).expect("clear");
    assert!(FilesystemStorage::load_app_config_at(&path)
        .expect("load")
        .io_bindings
        .is_empty());
}
