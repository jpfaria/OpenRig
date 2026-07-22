//! #792 audit #14: a read-modify-write of `config.yaml` must NOT wipe an
//! existing but CORRUPT config to defaults. `update_app_config_at` used
//! `load_app_config_at(...).unwrap_or_default()`, so a parse error turned into
//! a default that was then written back — silently destroying the user's real
//! config on the next setting change.

use infra_filesystem::FilesystemStorage;

#[test]
fn update_app_config_at_preserves_a_corrupt_config_instead_of_wiping_it() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("update_corrupt_guard");
    std::fs::create_dir_all(&dir).expect("mk test dir");
    let path = dir.join("config.yaml");
    // Existing file that is NOT valid AppConfig YAML.
    let corrupt = "this: is: not: a: valid: app: config: {[}\n";
    std::fs::write(&path, corrupt).expect("seed corrupt config");

    let result = FilesystemStorage::update_app_config_at(&path, |_config| {
        // Any mutation; the point is that the load must fail-closed first.
    });

    assert!(
        result.is_err(),
        "a corrupt config must abort the write (Err), not default-and-overwrite"
    );
    let after = std::fs::read_to_string(&path).expect("config still readable");
    assert_eq!(
        after, corrupt,
        "the corrupt config must be left untouched, never wiped to defaults"
    );
}
