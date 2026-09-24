use super::{bundled_presets_path, default_presets_path};

#[test]
fn presets_are_saved_under_the_users_data_folder() {
    // #978: the install folder is read-only (Program Files, a signed .app), so
    // a chain preset saved from a new or sidecar-less project was lost with a
    // success message. The default is the user's data folder, the one
    // openrig://paths already reported.
    assert_eq!(
        default_presets_path(),
        infra_filesystem::user_data_root().join("presets")
    );
}

#[test]
fn the_presets_that_ship_with_the_app_stay_in_the_install_folder() {
    assert_eq!(
        bundled_presets_path(),
        infra_filesystem::detect_data_root().join("presets")
    );
}
