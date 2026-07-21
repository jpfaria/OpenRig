use super::*;

pub(super) fn parse(yaml: &str) -> PluginManifest {
    serde_yaml::from_str(yaml).expect("manifest should parse")
}

#[test]
fn parses_nam_manifest() {
    let yaml = r#"
manifest_version: 1
id: my_preamp
display_name: My Preamp
type: preamp
backend: nam
parameters:
  - name: gain
    display_name: Gain
    values: [10, 20, 30]
captures:
  - values: { gain: 10 }
    file: captures/gain10.nam
  - values: { gain: 20 }
    file: captures/gain20.nam
  - values: { gain: 30 }
    file: captures/gain30.nam
"#;

    let m = parse(yaml);

    assert_eq!(m.manifest_version, 1);
    assert_eq!(m.id, "my_preamp");
    assert_eq!(m.block_type, BlockType::Preamp);
    match m.backend {
        Backend::Nam {
            parameters,
            captures,
        } => {
            assert_eq!(parameters.len(), 1);
            assert_eq!(parameters[0].name, "gain");
            assert!(matches!(
                parameters[0].values[0],
                ParameterValue::Number(value) if value == 10.0
            ));
            assert_eq!(captures.len(), 3);
            assert_eq!(captures[0].file, PathBuf::from("captures/gain10.nam"));
        }
        other => panic!("expected NAM backend, got {other:?}"),
    }
}

#[test]
fn parses_ir_manifest_with_no_parameters() {
    let yaml = r#"
manifest_version: 1
id: my_cab
display_name: My Cab
type: cab
backend: ir
captures:
  - values: {}
    file: ir/v30_4x12.wav
"#;

    let m = parse(yaml);

    assert_eq!(m.block_type, BlockType::Cab);
    match m.backend {
        Backend::Ir {
            parameters,
            captures,
        } => {
            assert!(parameters.is_empty(), "IR with no params");
            assert_eq!(captures.len(), 1);
        }
        other => panic!("expected IR backend, got {other:?}"),
    }
}

#[test]
fn parses_lv2_manifest_with_all_slots() {
    let yaml = r#"
manifest_version: 1
id: my_fuzz
display_name: My Fuzz
type: gain_pedal
backend: lv2
plugin_uri: http://example.com/plugins/my-fuzz
binaries:
  macos-universal: bundles/my-fuzz.lv2/macos-universal/my-fuzz.dylib
  windows-x86_64:  bundles/my-fuzz.lv2/windows-x86_64/my-fuzz.dll
  windows-aarch64: bundles/my-fuzz.lv2/windows-aarch64/my-fuzz.dll
  linux-x86_64:    bundles/my-fuzz.lv2/linux-x86_64/my-fuzz.so
  linux-aarch64:   bundles/my-fuzz.lv2/linux-aarch64/my-fuzz.so
"#;

    let m = parse(yaml);

    assert_eq!(m.block_type, BlockType::GainPedal);
    match m.backend {
        Backend::Lv2 {
            plugin_uri,
            binaries,
        } => {
            assert_eq!(plugin_uri, "http://example.com/plugins/my-fuzz");
            assert_eq!(binaries.len(), 5);
            assert!(binaries.contains_key(&Lv2Slot::MacosUniversal));
            assert!(binaries.contains_key(&Lv2Slot::LinuxAarch64));
        }
        other => panic!("expected LV2 backend, got {other:?}"),
    }
}

#[test]
fn parses_lv2_manifest_with_partial_slots() {
    let yaml = r#"
manifest_version: 1
id: linux_only_plugin
display_name: Linux Only
type: util
backend: lv2
plugin_uri: urn:example:linux-only
binaries:
  linux-x86_64: bundles/linux-only.lv2/linux-x86_64/plugin.so
  linux-aarch64: bundles/linux-only.lv2/linux-aarch64/plugin.so
"#;

    let m = parse(yaml);

    match m.backend {
        Backend::Lv2 { binaries, .. } => {
            assert_eq!(binaries.len(), 2);
            assert!(!binaries.contains_key(&Lv2Slot::MacosUniversal));
            assert!(!binaries.contains_key(&Lv2Slot::WindowsX86_64));
        }
        _ => panic!("expected LV2"),
    }
}

#[test]
fn rejects_unknown_backend() {
    let yaml = r#"
manifest_version: 1
id: bad
display_name: Bad
type: util
backend: vst3
"#;
    let result: Result<PluginManifest, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "unknown backend should be rejected");
}

#[test]
fn rejects_unknown_block_type() {
    let yaml = r#"
manifest_version: 1
id: bad
display_name: Bad
type: synthesizer
backend: nam
parameters: []
captures: []
"#;
    let result: Result<PluginManifest, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "unknown block type should be rejected");
}

#[test]
fn round_trip_nam_preserves_data() {
    let original = PluginManifest {
        manifest_version: 1,
        id: "round_trip".to_string(),
        display_name: "Round Trip".to_string(),
        author: Some("test".to_string()),
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
        block_type: BlockType::Preamp,
        backend: Backend::Nam {
            parameters: vec![GridParameter {
                name: "gain".to_string(),
                display_name: Some("Gain".to_string()),
                values: vec![ParameterValue::Number(10.0), ParameterValue::Number(20.0)],
            }],
            captures: vec![GridCapture {
                values: BTreeMap::from([("gain".to_string(), ParameterValue::Number(10.0))]),
                file: PathBuf::from("captures/g10.nam"),
                output_gain_db: None,
                noise_gate: None,
            }],
        },
    };

    let yaml = serde_yaml::to_string(&original).expect("serialize");
    let decoded: PluginManifest = serde_yaml::from_str(&yaml).expect("deserialize");
    assert_eq!(original, decoded);
}

#[test]
fn nam_manifest_surfaces_output_gain_db_calibration_to_engine() {
    // Issue #491: every shipped NAM manifest carries the measured loudness
    // offset under `output_gain_db` (dB, written by `nam_loudness_audit`).
    // The engine must read that exact key+unit, or the calibration is
    // silently dead (field deserializes to None, plugin plays at raw level).
    // This is a production-shaped manifest copied from `plugins/source/`.
    let yaml = r#"
manifest_version: 1
id: calibrated_amp
display_name: Calibrated Amp
type: amp
backend: nam
output_gain_db: 13.0556831
parameters:
  - name: gain
    values: [5]
captures:
  - values: { gain: 5 }
    file: captures/g5.nam
"#;

    let m = parse(yaml);

    assert_eq!(
        m.output_gain_db,
        Some(13.055_683),
        "manifest output_gain_db calibration must reach the engine in dB, unchanged"
    );
}

#[test]
fn vst3_manifest_deserializes_type_vst3_as_block_type_vst3() {
    // Issue #776: OpenRig-plugins ships catalog VST3 packages with `type: vst3`.
    // Without a `BlockType::Vst3` variant the manifest fails to deserialize and
    // the whole package is skipped, so the plugin never reaches the VST3 block
    // list. Production-shaped manifest copied from
    // `plugins/source/vst3/chow_centaur/manifest.yaml`.
    let yaml = r#"
manifest_version: 1
id: vst3_chow_centaur
display_name: ChowCentaur
brand: chowdsp
type: vst3
backend: vst3
bundle: bundles/ChowCentaur.vst3
parameters: []
"#;

    let m = parse(yaml);

    assert_eq!(m.block_type, BlockType::Vst3);
    assert!(matches!(m.backend, Backend::Vst3 { .. }));
}

// Issue #675 — per-capture / manifest-level noise gate defaults so a
// high-gain capture can ship with the gate regulated (the idle-hiss fix:
// high-gain captures amplify the input noise floor ~+32 dB).

#[test]
fn parses_nam_manifest_with_noise_gate_defaults() {
    let yaml = r#"
manifest_version: 1
id: vox_dirty
display_name: Vox Dirty
type: amp
backend: nam
noise_gate:
  enabled: true
  threshold_db: -60.0
parameters:
  - name: gain
    values: [5, 9]
captures:
  - values: { gain: 5 }
    file: captures/clean.nam
  - values: { gain: 9 }
    file: captures/hot.nam
    noise_gate:
      threshold_db: -55.0
"#;

    let m = parse(yaml);

    // Manifest-level default applies to all captures.
    let mg = m.noise_gate.as_ref().expect("manifest-level noise_gate");
    assert_eq!(mg.enabled, Some(true));
    assert_eq!(mg.threshold_db, Some(-60.0));

    match m.backend {
        Backend::Nam { captures, .. } => {
            // No per-capture override → None (inherits manifest level).
            assert_eq!(captures[0].noise_gate, None);
            // Per-capture override sets only the threshold; enabled stays
            // None so it inherits the manifest-level value.
            let cg = captures[1]
                .noise_gate
                .as_ref()
                .expect("per-capture noise_gate override");
            assert_eq!(cg.threshold_db, Some(-55.0));
            assert_eq!(cg.enabled, None);
        }
        other => panic!("expected NAM backend, got {other:?}"),
    }
}

#[test]
fn nam_manifest_without_noise_gate_is_none() {
    let yaml = r#"
manifest_version: 1
id: plain
display_name: Plain
type: amp
backend: nam
parameters:
  - name: gain
    values: [5]
captures:
  - values: { gain: 5 }
    file: captures/g5.nam
"#;

    let m = parse(yaml);

    assert_eq!(m.noise_gate, None);
    match m.backend {
        Backend::Nam { captures, .. } => assert_eq!(captures[0].noise_gate, None),
        other => panic!("expected NAM backend, got {other:?}"),
    }
}

