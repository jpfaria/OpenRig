//! Tests for `from_package` (issue #402).

use super::*;
use block_core::param::ParameterSet;
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
fn manifest_output_gain_db_is_applied_directly() {
    let manifest = nam_manifest(Some(-3.5));
    let params = ParameterSet::default();

    let effective =
        effective_plugin_params(&manifest, &params).expect("effective params should succeed");

    // No user knob — pure manifest correction on top of NAM defaults.
    let actual = effective.output_level_db;
    assert!(
        (actual - (-3.5)).abs() < 1e-6,
        "expected -3.5 dB (manifest correction), got {actual} dB"
    );
}

#[test]
fn manifest_output_gain_db_absent_yields_default_output_level() {
    let manifest = nam_manifest(None);
    let params = ParameterSet::default();

    let effective = effective_plugin_params(&manifest, &params).expect("ok");
    let actual = effective.output_level_db;
    assert!(
        (actual - DEFAULT_PLUGIN_PARAMS.output_level_db).abs() < 1e-6,
        "expected default output_level_db ({}), got {actual}",
        DEFAULT_PLUGIN_PARAMS.output_level_db
    );
}

#[test]
fn user_params_do_not_affect_output_level_db() {
    // Even if a stale preset still carries `output_db: 6.0`, we ignore
    // it — the user has no knob anymore (issue #402: "always 100%").
    use domain::value_objects::ParameterValue;
    let manifest = nam_manifest(Some(2.0));
    let mut params = ParameterSet::default();
    params.insert("output_db", ParameterValue::Float(99.0));

    let effective = effective_plugin_params(&manifest, &params).expect("ok");
    let actual = effective.output_level_db;
    assert!(
        (actual - 2.0).abs() < 1e-6,
        "expected manifest-only 2.0 dB, got {actual} dB"
    );
}
