use std::ffi::OsString;
use std::path::PathBuf;

use super::windows_config_base;

#[test]
fn appdata_wins_over_the_known_folder() {
    // #978: the tests (and anything else that re-points APPDATA) must land in
    // the directory they chose, like HOME/XDG already do on macOS/Linux.
    assert_eq!(
        windows_config_base(
            Some(OsString::from(r"D:\tmp\home")),
            Some(PathBuf::from(r"C:\Users\joao\AppData\Roaming"))
        ),
        Some(PathBuf::from(r"D:\tmp\home"))
    );
}

#[test]
fn the_known_folder_is_the_fallback() {
    assert_eq!(
        windows_config_base(None, Some(PathBuf::from(r"C:\Users\joao\AppData\Roaming"))),
        Some(PathBuf::from(r"C:\Users\joao\AppData\Roaming"))
    );
}

#[test]
fn an_empty_appdata_is_ignored() {
    assert_eq!(
        windows_config_base(Some(OsString::new()), Some(PathBuf::from(r"C:\Roaming"))),
        Some(PathBuf::from(r"C:\Roaming"))
    );
}
