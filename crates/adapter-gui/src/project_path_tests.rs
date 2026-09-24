use std::path::PathBuf;

use super::without_verbatim_prefix;

#[test]
fn windows_canonical_drive_path_loses_the_verbatim_prefix() {
    // fs::canonicalize on Windows returns `\\?\C:\...`, which the launcher
    // showed as-is and stored in the recent-projects list (#978).
    assert_eq!(
        without_verbatim_prefix(PathBuf::from(r"\\?\C:\Users\joao\rig.openrig")),
        PathBuf::from(r"C:\Users\joao\rig.openrig")
    );
}

#[test]
fn verbatim_unc_path_is_left_alone() {
    // `\\?\UNC\server\share` has no drive form to fall back to here.
    let unc = PathBuf::from(r"\\?\UNC\server\share\rig.openrig");
    assert_eq!(without_verbatim_prefix(unc.clone()), unc);
}

#[test]
fn ordinary_paths_are_unchanged() {
    for path in ["/Users/joao/rig.openrig", r"C:\rig.openrig", "rig.openrig"] {
        assert_eq!(
            without_verbatim_prefix(PathBuf::from(path)),
            PathBuf::from(path)
        );
    }
}
