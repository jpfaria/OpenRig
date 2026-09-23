use super::windows_install_root;

#[test]
fn install_root_accepts_dir_that_ships_assets() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    assert_eq!(
        windows_install_root(dir.path()),
        Some(dir.path().to_path_buf())
    );
}

#[test]
fn install_root_ignores_dir_without_assets() {
    // `target\debug` of a dev checkout: build output, no assets next to the exe,
    // so the working directory (the repo root) must stay the data root.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("deps")).unwrap();
    assert_eq!(windows_install_root(dir.path()), None);
}

#[test]
fn install_root_does_not_need_the_retired_libs_dir() {
    // What package-windows.ps1 has staged since #612: no `libs`.
    let dir = tempfile::tempdir().unwrap();
    for sub in ["assets", "plugins", "presets"] {
        std::fs::create_dir(dir.path().join(sub)).unwrap();
    }
    assert_eq!(
        windows_install_root(dir.path()),
        Some(dir.path().to_path_buf())
    );
}

#[test]
fn install_root_ignores_a_file_named_assets() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("assets"), b"not a directory").unwrap();
    assert_eq!(windows_install_root(dir.path()), None);
}
