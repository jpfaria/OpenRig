use std::path::PathBuf;

use super::*;
use crate::manifest::{BlockType, Lv2Slot};

fn nam_manifest(parameters: Vec<GridParameter>, captures: Vec<GridCapture>) -> PluginManifest {
    PluginManifest {
        manifest_version: 1,
        id: "test_plugin".to_string(),
        display_name: "Test Plugin".to_string(),
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
        block_type: BlockType::Preamp,
        backend: Backend::Nam {
            parameters,
            captures,
        },
    }
}

fn lv2_manifest() -> PluginManifest {
    PluginManifest {
        manifest_version: 1,
        id: "test_lv2".to_string(),
        display_name: "Test LV2".to_string(),
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
        block_type: BlockType::GainPedal,
        backend: Backend::Lv2 {
            plugin_uri: "urn:test:plugin".to_string(),
            binaries: BTreeMap::from([(
                Lv2Slot::LinuxX86_64,
                PathBuf::from("platform/linux-x86_64/plugin.so"),
            )]),
        },
    }
}

fn capture_with(values: &[(&str, f64)], file: &str) -> GridCapture {
    GridCapture {
        values: values
            .iter()
            .map(|(name, value)| ((*name).to_string(), ParameterValue::Number(*value)))
            .collect(),
        file: PathBuf::from(file),
    }
}

fn nums(raw: &[f64]) -> Vec<ParameterValue> {
    raw.iter().copied().map(ParameterValue::Number).collect()
}

#[test]
fn accepts_valid_nam_grid_1d() {
    let m = nam_manifest(
        vec![GridParameter {
            name: "gain".to_string(),
            display_name: None,
            values: nums(&[10.0, 20.0, 30.0]),
        }],
        vec![
            capture_with(&[("gain", 10.0)], "g10.nam"),
            capture_with(&[("gain", 20.0)], "g20.nam"),
            capture_with(&[("gain", 30.0)], "g30.nam"),
        ],
    );
    assert_eq!(validate_manifest(&m), Ok(()));
}

#[test]
fn accepts_valid_nam_grid_2d() {
    let m = nam_manifest(
        vec![
            GridParameter {
                name: "gain".to_string(),
                display_name: None,
                values: nums(&[10.0, 20.0]),
            },
            GridParameter {
                name: "volume".to_string(),
                display_name: None,
                values: nums(&[50.0, 60.0]),
            },
        ],
        vec![
            capture_with(&[("gain", 10.0), ("volume", 50.0)], "g10v50.nam"),
            capture_with(&[("gain", 10.0), ("volume", 60.0)], "g10v60.nam"),
            capture_with(&[("gain", 20.0), ("volume", 50.0)], "g20v50.nam"),
            capture_with(&[("gain", 20.0), ("volume", 60.0)], "g20v60.nam"),
        ],
    );
    assert_eq!(validate_manifest(&m), Ok(()));
}

#[test]
fn rejects_unsupported_version() {
    let mut m = nam_manifest(vec![], vec![]);
    m.manifest_version = 99;
    assert_eq!(
        validate_manifest(&m),
        Err(ValidationError::UnsupportedVersion {
            found: 99,
            max: MAX_SUPPORTED_VERSION,
        })
    );
}

#[test]
fn rejects_empty_id() {
    let mut m = lv2_manifest();
    m.id = "   ".to_string();
    assert_eq!(validate_manifest(&m), Err(ValidationError::EmptyId));
}

#[test]
fn rejects_empty_display_name() {
    let mut m = lv2_manifest();
    m.display_name = String::new();
    assert_eq!(
        validate_manifest(&m),
        Err(ValidationError::EmptyDisplayName)
    );
}

#[test]
fn rejects_lv2_with_no_slots() {
    let mut m = lv2_manifest();
    if let Backend::Lv2 {
        ref mut binaries, ..
    } = m.backend
    {
        binaries.clear();
    }
    assert_eq!(validate_manifest(&m), Err(ValidationError::NoLv2Slots));
}

#[test]
fn rejects_lv2_with_empty_uri() {
    let mut m = lv2_manifest();
    if let Backend::Lv2 {
        ref mut plugin_uri, ..
    } = m.backend
    {
        *plugin_uri = String::new();
    }
    assert_eq!(
        validate_manifest(&m),
        Err(ValidationError::EmptyLv2PluginUri)
    );
}

#[test]
fn rejects_capture_with_unknown_parameter() {
    let m = nam_manifest(
        vec![GridParameter {
            name: "gain".to_string(),
            display_name: None,
            values: nums(&[10.0]),
        }],
        vec![capture_with(
            &[("gain", 10.0), ("typo_param", 1.0)],
            "x.nam",
        )],
    );
    let err = validate_manifest(&m).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::UnknownCaptureParameter { ref parameter, .. } if parameter == "typo_param"
    ));
}

#[test]
fn rejects_capture_with_value_not_in_grid() {
    let m = nam_manifest(
        vec![GridParameter {
            name: "gain".to_string(),
            display_name: None,
            values: nums(&[10.0, 20.0]),
        }],
        vec![
            capture_with(&[("gain", 10.0)], "g10.nam"),
            capture_with(&[("gain", 99.0)], "g99.nam"),
        ],
    );
    let err = validate_manifest(&m).unwrap_err();
    match err {
        ValidationError::InvalidCaptureValue { value, .. } => {
            assert_eq!(value, ParameterValue::Number(99.0));
        }
        other => panic!("expected InvalidCaptureValue, got {other:?}"),
    }
}

#[test]
fn rejects_capture_missing_a_parameter() {
    let m = nam_manifest(
        vec![
            GridParameter {
                name: "gain".to_string(),
                display_name: None,
                values: nums(&[10.0]),
            },
            GridParameter {
                name: "volume".to_string(),
                display_name: None,
                values: nums(&[50.0]),
            },
        ],
        vec![capture_with(&[("gain", 10.0)], "g10.nam")],
    );
    let err = validate_manifest(&m).unwrap_err();
    assert!(matches!(
        err,
        ValidationError::MissingCaptureParameter { ref parameter, .. } if parameter == "volume"
    ));
}

#[test]
fn accepts_sparse_grid() {
    // Real-world cabs (e.g. block-cab/ir_ampeg_svt_8x10) declare a 2D
    // mic × position parameter grid but ship only ~8 of 21 cells —
    // only the combinations actually captured. Validator must accept.
    let m = nam_manifest(
        vec![
            GridParameter {
                name: "mic".to_string(),
                display_name: None,
                values: vec![
                    ParameterValue::Text("d6".to_string()),
                    ParameterValue::Text("57".to_string()),
                    ParameterValue::Text("4033".to_string()),
                ],
            },
            GridParameter {
                name: "position".to_string(),
                display_name: None,
                values: vec![
                    ParameterValue::Text("ah".to_string()),
                    ParameterValue::Text("a107".to_string()),
                    ParameterValue::Text("svt_di".to_string()),
                ],
            },
        ],
        // Sparse: only 3 of 9 combinations.
        vec![
            GridCapture {
                values: BTreeMap::from([
                    ("mic".to_string(), ParameterValue::Text("d6".to_string())),
                    (
                        "position".to_string(),
                        ParameterValue::Text("ah".to_string()),
                    ),
                ]),
                file: PathBuf::from("a.wav"),
            },
            GridCapture {
                values: BTreeMap::from([
                    ("mic".to_string(), ParameterValue::Text("57".to_string())),
                    (
                        "position".to_string(),
                        ParameterValue::Text("ah".to_string()),
                    ),
                ]),
                file: PathBuf::from("b.wav"),
            },
            GridCapture {
                values: BTreeMap::from([
                    ("mic".to_string(), ParameterValue::Text("4033".to_string())),
                    (
                        "position".to_string(),
                        ParameterValue::Text("a107".to_string()),
                    ),
                ]),
                file: PathBuf::from("c.wav"),
            },
        ],
    );
    assert_eq!(validate_manifest(&m), Ok(()));
}

#[test]
fn rejects_empty_grid_when_parameters_declared() {
    let m = nam_manifest(
        vec![GridParameter {
            name: "gain".to_string(),
            display_name: None,
            values: nums(&[10.0, 20.0]),
        }],
        vec![],
    );
    assert_eq!(
        validate_manifest(&m),
        Err(ValidationError::EmptyCaptureGrid)
    );
}

#[test]
fn rejects_duplicate_captures() {
    let m = nam_manifest(
        vec![GridParameter {
            name: "gain".to_string(),
            display_name: None,
            values: nums(&[10.0, 20.0]),
        }],
        vec![
            capture_with(&[("gain", 10.0)], "g10a.nam"),
            capture_with(&[("gain", 10.0)], "g10b.nam"),
        ],
    );
    assert_eq!(
        validate_manifest(&m),
        Err(ValidationError::DuplicateCaptures)
    );
}

#[test]
fn rejects_parameter_with_no_values() {
    let m = nam_manifest(
        vec![GridParameter {
            name: "gain".to_string(),
            display_name: None,
            values: vec![],
        }],
        vec![],
    );
    assert_eq!(
        validate_manifest(&m),
        Err(ValidationError::EmptyParameterValues {
            name: "gain".to_string(),
        })
    );
}

#[test]
fn accepts_ir_with_no_parameters_and_one_capture() {
    let m = PluginManifest {
        manifest_version: 1,
        id: "ir_cab".to_string(),
        display_name: "IR Cab".to_string(),
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
        block_type: BlockType::Cab,
        backend: Backend::Ir {
            parameters: vec![],
            captures: vec![capture_with(&[], "ir/cab.wav")],
        },
    };
    assert_eq!(validate_manifest(&m), Ok(()));
}
