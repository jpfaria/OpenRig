use super::*;
use crate::manifest::{Backend, BlockType, GridCapture, GridParameter, ParameterValue};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn temp_package(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openrig-version-store-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn manifest(axis: &str) -> PluginManifest {
    PluginManifest {
        manifest_version: 1,
        id: "user_cab".into(),
        display_name: "User Cab".into(),
        author: None,
        description: None,
        inspired_by: None,
        brand: None,
        thumbnail: None,
        photo: None,
        screenshot: None,
        brand_logo: None,
        license: None,
        homepage: None,
        sources: None,
        output_gain_db: None,
        noise_gate: None,
        architecture: None,
        block_type: BlockType::Cab,
        backend: Backend::Ir {
            parameters: vec![GridParameter {
                name: axis.into(),
                display_name: None,
                values: vec![ParameterValue::Text("a".into())],
            }],
            captures: vec![GridCapture {
                values: BTreeMap::from([(axis.to_string(), ParameterValue::Text("a".into()))]),
                file: "captures/000.wav".into(),
                output_gain_db: None,
                noise_gate: None,
            }],
        },
    }
}

#[test]
fn a_package_without_saved_versions_lists_none() {
    let dir = temp_package("none");
    assert!(version_numbers(&dir).is_empty());
    assert!(read_version(&dir, 1).is_none());
}

#[test]
fn saved_versions_read_back_in_ascending_order() {
    let dir = temp_package("order");
    write_version(&dir, 2, &manifest("mic")).unwrap();
    write_version(&dir, 1, &manifest("preset")).unwrap();
    write_version(&dir, 10, &manifest("position")).unwrap();

    assert_eq!(version_numbers(&dir), vec![1, 2, 10]);
    assert_eq!(read_version(&dir, 1).unwrap(), manifest("preset"));
    assert_eq!(read_version(&dir, 10).unwrap(), manifest("position"));
}

#[test]
fn stray_files_in_the_versions_folder_are_not_versions() {
    let dir = temp_package("stray");
    write_version(&dir, 1, &manifest("mic")).unwrap();
    std::fs::write(dir.join(VERSIONS_DIR).join("notes.txt"), "x").unwrap();
    std::fs::write(dir.join(VERSIONS_DIR).join("abc.yaml"), "x").unwrap();

    assert_eq!(version_numbers(&dir), vec![1]);
}

#[test]
fn a_saved_version_is_never_read_as_a_package() {
    let dir = temp_package("discover");
    write_version(&dir, 1, &manifest("mic")).unwrap();
    assert!(crate::discover(&dir).unwrap().is_empty());
}
