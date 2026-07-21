//! Manifest NAM/enum-parameter + validation tests (issue #792 split from
//! manifest_tests.rs). Shares the parse() helper via super::tests.

use super::tests::parse;
use super::*;

#[test]
fn parses_nam_manifest_with_enum_string_parameters() {
    let yaml = r#"
manifest_version: 1
id: ampeg_svt
display_name: SVT Classic
type: amp
backend: nam
parameters:
  - name: tone
    values: [standard, ultra_hi, ultra_lo]
  - name: mic
    values: [md421, sm57]
captures:
  - values: { tone: standard, mic: md421 }
    file: captures/svt_standard_md421.nam
  - values: { tone: standard, mic: sm57 }
    file: captures/svt_standard_sm57.nam
  - values: { tone: ultra_hi, mic: md421 }
    file: captures/svt_ultra_hi_md421.nam
  - values: { tone: ultra_hi, mic: sm57 }
    file: captures/svt_ultra_hi_sm57.nam
  - values: { tone: ultra_lo, mic: md421 }
    file: captures/svt_ultra_lo_md421.nam
  - values: { tone: ultra_lo, mic: sm57 }
    file: captures/svt_ultra_lo_sm57.nam
"#;

    let m = parse(yaml);

    match m.backend {
        Backend::Nam {
            parameters,
            captures,
        } => {
            assert_eq!(parameters.len(), 2);
            assert_eq!(parameters[0].name, "tone");
            assert!(matches!(
                parameters[0].values[0],
                ParameterValue::Text(ref s) if s == "standard"
            ));
            assert_eq!(captures.len(), 6);
        }
        other => panic!("expected NAM backend, got {other:?}"),
    }
}

// Issue #514 — per-capture output_gain_db on IR captures.

#[test]
fn parses_ir_manifest_with_per_capture_output_gain_db() {
    let yaml = r#"
manifest_version: 1
id: my_body
display_name: My Body
type: body
backend: ir
parameters:
  - name: voicing
    display_name: Voicing
    values: [bright, dark]
captures:
  - values: { voicing: bright }
    file: ir/bright.wav
    output_gain_db: -3.5
  - values: { voicing: dark }
    file: ir/dark.wav
    output_gain_db: 1.25
"#;

    let m = parse(yaml);

    match m.backend {
        Backend::Ir { captures, .. } => {
            assert_eq!(captures.len(), 2);
            assert_eq!(captures[0].output_gain_db, Some(-3.5));
            assert_eq!(captures[1].output_gain_db, Some(1.25));
        }
        other => panic!("expected IR backend, got {other:?}"),
    }
}

#[test]
fn ir_capture_without_output_gain_db_is_none() {
    let yaml = r#"
manifest_version: 1
id: my_cab
display_name: My Cab
type: cab
backend: ir
captures:
  - values: {}
    file: ir/cab.wav
"#;

    let m = parse(yaml);

    match m.backend {
        Backend::Ir { captures, .. } => {
            assert_eq!(captures.len(), 1);
            assert_eq!(captures[0].output_gain_db, None);
        }
        other => panic!("expected IR backend, got {other:?}"),
    }
}

#[test]
fn per_capture_output_gain_db_round_trips_through_serde() {
    let yaml = r#"
manifest_version: 1
id: rt
display_name: Round Trip
type: body
backend: ir
captures:
  - values: {}
    file: ir/one.wav
    output_gain_db: -7.875
"#;
    let m = parse(yaml);
    let serialized = serde_yaml::to_string(&m).expect("manifest serializes");
    let reparsed: PluginManifest = serde_yaml::from_str(&serialized).expect("re-parse");

    match reparsed.backend {
        Backend::Ir { captures, .. } => {
            assert_eq!(captures[0].output_gain_db, Some(-7.875));
        }
        other => panic!("expected IR backend, got {other:?}"),
    }
}

// Issue #650 — per-plugin NAM architecture (A1/A2) declared on the manifest.
// Every NAM plugin is uniform: all captures share one architecture. The field
// is a summary so the catalog can label/filter without parsing every .nam.

#[test]
fn parses_nam_manifest_with_architecture_a2() {
    let yaml = r#"
manifest_version: 1
id: slimmable_amp
display_name: Slimmable Amp
type: amp
backend: nam
architecture: A2
parameters:
  - name: gain
    values: [5]
captures:
  - values: { gain: 5 }
    file: captures/g5.nam
"#;

    let m = parse(yaml);

    assert_eq!(m.architecture, Some(NamArchitecture::A2));
}

#[test]
fn parses_nam_manifest_with_architecture_a1() {
    let yaml = r#"
manifest_version: 1
id: wavenet_amp
display_name: WaveNet Amp
type: amp
backend: nam
architecture: A1
parameters:
  - name: gain
    values: [5]
captures:
  - values: { gain: 5 }
    file: captures/g5.nam
"#;

    let m = parse(yaml);

    assert_eq!(m.architecture, Some(NamArchitecture::A1));
}

#[test]
fn ir_manifest_without_architecture_is_none() {
    let yaml = r#"
manifest_version: 1
id: my_cab
display_name: My Cab
type: cab
backend: ir
captures:
  - values: {}
    file: ir/cab.wav
"#;

    let m = parse(yaml);

    assert_eq!(m.architecture, None, "IR plugins never carry architecture");
}

#[test]
fn legacy_nam_manifest_without_architecture_is_none() {
    // A pre-#650 NAM manifest has no `architecture` key — it must still parse,
    // deserializing the field to None (no error).
    let yaml = r#"
manifest_version: 1
id: legacy_amp
display_name: Legacy Amp
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

    assert_eq!(m.architecture, None);
}

#[test]
fn architecture_round_trips_through_serde() {
    let yaml = r#"
manifest_version: 1
id: rt_arch
display_name: Round Trip Arch
type: amp
backend: nam
architecture: A2
parameters:
  - name: gain
    values: [5]
captures:
  - values: { gain: 5 }
    file: captures/g5.nam
"#;
    let m = parse(yaml);
    let serialized = serde_yaml::to_string(&m).expect("manifest serializes");
    let reparsed: PluginManifest = serde_yaml::from_str(&serialized).expect("re-parse");

    assert_eq!(reparsed.architecture, Some(NamArchitecture::A2));
}

// Issue #675 — pure precedence resolver for the manifest noise gate:
// per-capture override wins per field over the manifest-level default.

#[test]
fn resolve_noise_gate_picks_per_capture_over_manifest_per_field() {
    let manifest = ManifestNoiseGate {
        enabled: Some(true),
        threshold_db: Some(-60.0),
    };
    let capture = ManifestNoiseGate {
        enabled: None,
        threshold_db: Some(-55.0),
    };
    let (enabled, threshold) = resolve_noise_gate(Some(&capture), Some(&manifest));
    assert_eq!(
        enabled,
        Some(true),
        "enabled inherits the manifest-level value"
    );
    assert_eq!(
        threshold,
        Some(-55.0),
        "threshold comes from the per-capture override"
    );
}

#[test]
fn resolve_noise_gate_is_none_when_neither_sets_a_field() {
    let (enabled, threshold) = resolve_noise_gate(None, None);
    assert_eq!(enabled, None);
    assert_eq!(threshold, None);
}

#[test]
fn vst3_manifest_exposes_group_map_keyed_by_vst3_id() {
    // #780: a VST3 package declares which tab each parameter belongs to.
    // The app overlays these onto the live parameters by `vst3_id`.
    let yaml = r#"
manifest_version: 1
id: chow_centaur
display_name: Chow Centaur
type: vst3
backend: vst3
bundle: ChowCentaur.vst3
parameters:
  - name: gain
    vst3_id: 0
    min: 0.0
    max: 100.0
    default: 50.0
    group: Tone
  - name: level
    vst3_id: 1
    min: 0.0
    max: 100.0
    default: 50.0
    group: Tone
  - name: mode
    vst3_id: 5
    min: 0.0
    max: 100.0
    default: 0.0
    group: Voicing
  - name: bypass
    vst3_id: 9
    min: 0.0
    max: 1.0
    default: 0.0
"#;
    let manifest = parse(yaml);
    let map = manifest.vst3_group_map();
    assert_eq!(map.get(&0).map(String::as_str), Some("Tone"));
    assert_eq!(map.get(&1).map(String::as_str), Some("Tone"));
    assert_eq!(map.get(&5).map(String::as_str), Some("Voicing"));
    // A parameter with no declared group is absent — the app groups it
    // dynamically, it does not land in the manifest map.
    assert!(!map.contains_key(&9));
    assert_eq!(map.len(), 3);
}
