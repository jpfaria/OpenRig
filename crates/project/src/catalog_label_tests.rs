//! Tests for `project::catalog_label`. Issue #650.

use super::package_type_label;
use plugin_loader::manifest::{
    Backend, BlockType, GridCapture, GridParameter, NamArchitecture, ParameterValue,
    PluginManifest,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn nam_manifest(architecture: Option<NamArchitecture>) -> PluginManifest {
    PluginManifest {
        manifest_version: 1,
        id: "amp".to_string(),
        display_name: "Amp".to_string(),
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
        architecture,
        block_type: BlockType::Amp,
        backend: Backend::Nam {
            parameters: vec![GridParameter {
                name: "gain".to_string(),
                display_name: None,
                values: vec![ParameterValue::Number(5.0)],
            }],
            captures: vec![GridCapture {
                values: BTreeMap::from([("gain".to_string(), ParameterValue::Number(5.0))]),
                file: PathBuf::from("captures/g5.nam"),
                output_gain_db: None,
            }],
        },
    }
}

fn ir_manifest() -> PluginManifest {
    PluginManifest {
        block_type: BlockType::Cab,
        backend: Backend::Ir {
            parameters: vec![],
            captures: vec![GridCapture {
                values: BTreeMap::new(),
                file: PathBuf::from("ir/cab.wav"),
                output_gain_db: None,
            }],
        },
        ..nam_manifest(None)
    }
}

#[test]
fn nam_a2_label_is_nam_slash_a2() {
    assert_eq!(
        package_type_label(&nam_manifest(Some(NamArchitecture::A2))),
        "NAM/A2"
    );
}

#[test]
fn nam_a1_label_is_nam_slash_a1() {
    assert_eq!(
        package_type_label(&nam_manifest(Some(NamArchitecture::A1))),
        "NAM/A1"
    );
}

#[test]
fn nam_without_architecture_label_is_plain_nam() {
    assert_eq!(package_type_label(&nam_manifest(None)), "NAM");
}

#[test]
fn ir_label_is_plain_ir_ignoring_architecture() {
    assert_eq!(package_type_label(&ir_manifest()), "IR");
}
