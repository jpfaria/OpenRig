//! Where the user's own files live: one dot folder in the home directory on
//! macOS and Linux, holding the system config at its root, the presets and the
//! projects beside it.

use std::path::PathBuf;

use crate::{default_presets_path, default_projects_path, user_data_root, FilesystemStorage};

#[cfg(not(target_os = "windows"))]
#[test]
fn the_user_folder_is_dot_openrig_in_the_home_directory() {
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME is set"));
    assert_eq!(user_data_root(), home.join(".openrig"));
}

#[cfg(target_os = "windows")]
#[test]
fn the_user_folder_is_openrig_in_appdata() {
    let appdata = PathBuf::from(std::env::var_os("APPDATA").expect("APPDATA is set"));
    assert_eq!(user_data_root(), appdata.join("OpenRig"));
}

#[test]
fn the_system_config_sits_at_the_root_of_the_user_folder() {
    assert_eq!(
        FilesystemStorage::app_config_path().unwrap(),
        user_data_root().join("config.yaml")
    );
    assert_eq!(
        FilesystemStorage::gui_settings_path().unwrap(),
        user_data_root().join("gui-settings.yaml")
    );
}

#[test]
fn the_midi_files_sit_at_the_root_of_the_user_folder() {
    assert_eq!(
        FilesystemStorage::midi_map_path().unwrap(),
        user_data_root().join("midi-map.yaml")
    );
    assert_eq!(
        FilesystemStorage::midi_profile_path().unwrap(),
        user_data_root().join("midi-profile.yaml")
    );
    assert_eq!(
        FilesystemStorage::midi_bindings_path().unwrap(),
        user_data_root().join("midi-bindings.yaml")
    );
}

#[test]
fn presets_default_to_the_presets_folder_of_the_user_folder() {
    assert_eq!(default_presets_path(), user_data_root().join("presets"));
}

#[test]
fn projects_default_to_the_projects_folder_of_the_user_folder() {
    assert_eq!(default_projects_path(), user_data_root().join("projects"));
}
