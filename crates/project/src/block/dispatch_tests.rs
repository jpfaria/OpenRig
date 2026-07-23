use super::*;
use plugin_loader::manifest::{
    Backend, BlockType, GridCapture, GridParameter, ParameterValue, PluginManifest,
};
use plugin_loader::LoadedPackage;
use std::path::PathBuf;

fn nam_amp_package(
    id: &str,
    display_name: &str,
    axes: Vec<GridParameter>,
    captures: Vec<GridCapture>,
) -> LoadedPackage {
    LoadedPackage {
        root: PathBuf::from("/fake"),
        manifest: PluginManifest {
            manifest_version: 1,
            id: id.into(),
            display_name: display_name.into(),
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
            block_type: BlockType::Amp,
            backend: Backend::Nam {
                parameters: axes,
                captures,
            },
        },
    }
}

fn nam_package_with_axes() -> LoadedPackage {
    nam_amp_package(
        "nam_test_amp",
        "Test NAM Amp",
        vec![GridParameter {
            name: "channel".into(),
            display_name: None,
            values: vec![
                ParameterValue::Text("a".into()),
                ParameterValue::Text("b".into()),
            ],
        }],
        vec![],
    )
}

#[test]
fn nam_synthesized_schema_exposes_output_db_knob() {
    // Issue #496 reversed #402: when audit-side `output_gain_db`
    // values are zeroed (or absent), the user has no way to add
    // makeup gain on a quiet capture — the chain plays at the raw
    // model output, which is typically far below realistic amp
    // level. Exposing the Output knob gives the user manual control;
    // when a hot `output_gain_db` IS present in the manifest, it is
    // still applied automatically (the two coexist additively).
    let pkg = nam_package_with_axes();
    let specs = synthesize_parameters_from_manifest(&pkg);
    assert!(
        specs.iter().any(|s| s.path == "output_db"),
        "NAM schema must include `output_db` so the user can add \
         makeup gain when the manifest is zero; got params: {:?}",
        specs.iter().map(|s| &s.path).collect::<Vec<_>>()
    );
    assert!(
        specs.iter().any(|s| s.path == "input_db"),
        "NAM schema must include `input_db` (always was)"
    );
}

#[test]
fn nam_a2_synthesized_schema_exposes_slim_knob() {
    // Issue #657: A2 (SlimmableContainer) models expose a runtime
    // `slim` size knob wired to SetSlimmableSize.
    use plugin_loader::manifest::NamArchitecture;
    let mut pkg = nam_package_with_axes();
    pkg.manifest.architecture = Some(NamArchitecture::A2);
    let specs = synthesize_parameters_from_manifest(&pkg);
    assert!(
        specs.iter().any(|s| s.path == "slim"),
        "NAM/A2 schema must expose the `slim` knob; got: {:?}",
        specs.iter().map(|s| &s.path).collect::<Vec<_>>()
    );
}

#[test]
fn nam_a1_and_legacy_synthesized_schema_have_no_slim_knob() {
    // A1 models are not slimmable, and pre-#650 manifests have no
    // architecture at all — neither exposes the slim knob (issue #657).
    use plugin_loader::manifest::NamArchitecture;
    let mut a1 = nam_package_with_axes();
    a1.manifest.architecture = Some(NamArchitecture::A1);
    assert!(
        !synthesize_parameters_from_manifest(&a1)
            .iter()
            .any(|s| s.path == "slim"),
        "A1 NAM must NOT expose the slim knob (not slimmable)"
    );
    let legacy = nam_package_with_axes(); // architecture: None
    assert!(
        !synthesize_parameters_from_manifest(&legacy)
            .iter()
            .any(|s| s.path == "slim"),
        "legacy NAM (no architecture) must NOT expose the slim knob"
    );
}

fn ir_package_with_capture_audit(first_audit_db: Option<f32>) -> LoadedPackage {
    LoadedPackage {
        root: PathBuf::from("/fake"),
        manifest: PluginManifest {
            manifest_version: 1,
            id: "ir_test_body".into(),
            display_name: "Test IR".into(),
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
                    name: "position".into(),
                    display_name: None,
                    values: vec![
                        ParameterValue::Text("a".into()),
                        ParameterValue::Text("b".into()),
                    ],
                }],
                captures: vec![
                    GridCapture {
                        values: [("position".to_string(), ParameterValue::Text("a".into()))]
                            .into_iter()
                            .collect(),
                        file: "a.wav".into(),
                        output_gain_db: first_audit_db,
                        noise_gate: None,
                    },
                    GridCapture {
                        values: [("position".to_string(), ParameterValue::Text("b".into()))]
                            .into_iter()
                            .collect(),
                        file: "b.wav".into(),
                        output_gain_db: Some(-10.0),
                        noise_gate: None,
                    },
                ],
            },
        },
    }
}

#[test]
fn ir_synthesized_schema_exposes_output_db_knob_in_decibels() {
    // Issue #655: IR blocks need a user-adjustable Output Level knob
    // (mirroring NAM) so resonant body IRs whose audit baseline cut
    // them far down can be brought back up. It must be a dB control.
    let pkg = ir_package_with_capture_audit(Some(-22.9));
    let specs = synthesize_parameters_from_manifest(&pkg);
    let output_db = specs
        .iter()
        .find(|s| s.path == "output_db")
        .expect("IR schema must include `output_db` so the user can adjust output level");
    assert_eq!(
        output_db.unit,
        block_core::param::ParameterUnit::Decibels,
        "output_db must be a decibel control"
    );
}

#[test]
fn ir_output_db_default_seeds_from_first_capture_audit() {
    // The knob's default mirrors the engine's actual baseline so a
    // freshly created IR block (born at the first capture) shows the
    // real applied offset, not 0 dB. Volume invariant #10.
    let pkg = ir_package_with_capture_audit(Some(-22.9));
    let specs = synthesize_parameters_from_manifest(&pkg);
    let output_db = specs.iter().find(|s| s.path == "output_db").unwrap();
    assert_eq!(
        output_db.default_value,
        Some(domain::value_objects::ParameterValue::Float(-22.9)),
        "output_db default must be the first capture's audit baseline"
    );
}

fn nam_package_with_emoji_labels() -> LoadedPackage {
    // Real-world Bogner Ecstasy capture grid — `display_name` and
    // every `Text` value carry a leading emoji. Reproduces the
    // tofu/black-square symptom from issue #424.
    // Both cabinet values are capture-backed so the axis survives the
    // #649 dead-axis filter and the emoji stripping is exercised on a
    // rendered control.
    nam_amp_package(
        "nam_bogner_ecstasy",
        "Bogner Ecstasy",
        vec![GridParameter {
            name: "cabinet".into(),
            display_name: Some("📦 Cabinet".into()),
            values: vec![
                ParameterValue::Text("✋ 4X12".into()),
                ParameterValue::Text("🔥 2X12".into()),
            ],
        }],
        vec![
            GridCapture {
                values: [(
                    "cabinet".to_string(),
                    ParameterValue::Text("✋ 4X12".into()),
                )]
                .into_iter()
                .collect(),
                file: "4x12.nam".into(),
                output_gain_db: None,
                noise_gate: None,
            },
            GridCapture {
                values: [(
                    "cabinet".to_string(),
                    ParameterValue::Text("🔥 2X12".into()),
                )]
                .into_iter()
                .collect(),
                file: "2x12.nam".into(),
                output_gain_db: None,
                noise_gate: None,
            },
        ],
    )
}

#[test]
fn nam_grid_parameter_label_strips_emoji_for_ui() {
    // Issue #424: shipped fonts (Bebas Neue, Inter, Permanent
    // Marker, …) carry no emoji glyphs; macOS cascades to Apple
    // Color Emoji, Windows / Linux do not, so emojis render as
    // tofu in the BlockEditorPanel selectors.
    let pkg = nam_package_with_emoji_labels();
    let specs = synthesize_parameters_from_manifest(&pkg);
    let cabinet = specs
        .iter()
        .find(|s| s.path == "cabinet")
        .expect("cabinet axis must be in synthesized schema");
    assert_eq!(
        cabinet.label, "Cabinet",
        "axis display_name must be emoji-free for UI rendering"
    );
    let block_core::param::ParameterDomain::Enum { options } = &cabinet.domain else {
        panic!(
            "text-valued grid axis must become an enum, got {:?}",
            cabinet.domain
        );
    };
    let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["4X12", "2X12"],
        "option labels must be emoji-free; raw values stay for capture lookup"
    );
    // Pinned: storage-side values keep the original strings so
    // `resolve_capture` can still match user selections to the
    // manifest's `captures[].values`.
    let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
    assert_eq!(
        values,
        vec!["✋ 4X12", "🔥 2X12"],
        "raw values must be preserved for capture lookup / persistence"
    );
}
