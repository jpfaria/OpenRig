//! Tests for `from_package` (issue #402 phase 2).

use super::*;
use block_core::param::ParameterSet;
use domain::value_objects::ParameterValue;
use plugin_loader::manifest::{Backend, BlockType, GridCapture, PluginManifest};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn nam_manifest(output_gain_db: Option<f32>) -> PluginManifest {
    PluginManifest {
        manifest_version: 1,
        id: "nam_audited".to_string(),
        display_name: "Audited NAM".to_string(),
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
        output_gain_db,
        block_type: BlockType::Amp,
        backend: Backend::Nam {
            parameters: vec![],
            captures: vec![GridCapture {
                values: BTreeMap::new(),
                file: PathBuf::from("captures/x.nam"),
            }],
        },
    }
}

#[test]
fn manifest_output_gain_db_is_summed_onto_user_output_db() {
    let manifest = nam_manifest(Some(-3.5));
    let mut params = ParameterSet::default();
    params.insert("output_db", ParameterValue::Float(6.0));

    let effective =
        effective_plugin_params(&manifest, &params).expect("effective params should succeed");

    // 6.0 (user) + (-3.5) (manifest correction) = 2.5
    let actual = effective.output_level_db;
    assert!(
        (actual - 2.5).abs() < 1e-6,
        "expected 2.5 dB, got {actual} dB"
    );
}

#[test]
fn manifest_output_gain_db_absent_is_treated_as_zero() {
    let manifest = nam_manifest(None);
    let mut params = ParameterSet::default();
    params.insert("output_db", ParameterValue::Float(4.0));

    let effective = effective_plugin_params(&manifest, &params).expect("ok");
    let actual = effective.output_level_db;
    assert!(
        (actual - 4.0).abs() < 1e-6,
        "expected 4.0 dB (user only), got {actual} dB"
    );
}

#[test]
fn manifest_output_gain_db_alone_when_user_omits_output_db() {
    let manifest = nam_manifest(Some(2.0));
    let params = ParameterSet::default();

    let effective = effective_plugin_params(&manifest, &params).expect("ok");
    let actual = effective.output_level_db;
    assert!(
        (actual - 2.0).abs() < 1e-6,
        "expected 2.0 dB (manifest only), got {actual} dB"
    );
}
