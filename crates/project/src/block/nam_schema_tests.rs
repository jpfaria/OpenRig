use super::super::dispatch::synthesize_parameters_from_manifest;
use super::*;
use plugin_loader::manifest::{
    Backend, BlockType, GridCapture, GridParameter, NamArchitecture, ParameterValue, PluginManifest,
};
use plugin_loader::LoadedPackage;
use std::path::PathBuf;

pub(super) fn nam_amp_package(
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

/// Axis declared but no capture references it — `effective_grid_axes`
/// drops it (issue #649), so this package is engine-knobs only.
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

/// Same axis, this time capture-backed, so it survives into the schema.
fn nam_package_with_live_axis() -> LoadedPackage {
    let capture = |channel: &str, file: &str| GridCapture {
        values: [(
            "channel".to_string(),
            ParameterValue::Text(channel.to_string()),
        )]
        .into_iter()
        .collect(),
        file: file.into(),
        output_gain_db: None,
        noise_gate: None,
    };
    nam_amp_package(
        "nam_test_amp",
        "Test NAM Amp",
        vec![GridParameter {
            name: "channel".into(),
            display_name: None,
            values: vec![
                ParameterValue::Text("clean".into()),
                ParameterValue::Text("od1".into()),
            ],
        }],
        vec![
            capture("clean", "captures/clean.nam"),
            capture("od1", "captures/od1.nam"),
        ],
    )
}

fn group_of<'a>(specs: &'a [block_core::param::ParameterSpec], path: &str) -> Option<&'a str> {
    specs
        .iter()
        .find(|s| s.path == path)
        .unwrap_or_else(|| panic!("NAM schema must expose `{path}`"))
        .group
        .as_deref()
}

#[test]
fn nam_schema_splits_capture_axes_from_engine_defaults() {
    // Issue #786: the editor renders one tab per parameter group, and
    // for NAM the split needs no authoring — whatever the manifest
    // declares under `parameters:` selects the capture, everything else
    // is an engine control every NAM has. Covers A2 (the slim knob is
    // an engine control too).
    let mut pkg = nam_package_with_live_axis();
    pkg.manifest.architecture = Some(NamArchitecture::A2);
    let specs = synthesize_parameters_from_manifest(&pkg);
    assert_eq!(
        group_of(&specs, "channel"),
        Some(NAM_CAPTURE_GROUP),
        "a manifest axis belongs to the Capture tab"
    );
    for (engine, tab) in [
        ("input_db", nam::params::AMP_GROUP),
        ("output_db", nam::params::AMP_GROUP),
        ("slim", nam::params::AMP_GROUP),
        ("noise_gate.enabled", nam::params::NOISE_GATE_GROUP),
        ("noise_gate.threshold_db", nam::params::NOISE_GATE_GROUP),
        ("eq.enabled", nam::params::EQ_GROUP),
        ("eq.bass", nam::params::EQ_GROUP),
        ("eq.middle", nam::params::EQ_GROUP),
        ("eq.treble", nam::params::EQ_GROUP),
    ] {
        assert_eq!(
            group_of(&specs, engine),
            Some(tab),
            "engine default `{engine}` belongs to the {tab} tab"
        );
    }
    let tabs = specs.iter().fold(Vec::new(), |mut acc, spec| {
        let group = spec.group.as_deref().unwrap_or_default();
        if !acc.contains(&group) {
            acc.push(group);
        }
        acc
    });
    assert_eq!(
        tabs,
        vec![
            NAM_CAPTURE_GROUP,
            nam::params::AMP_GROUP,
            nam::params::NOISE_GATE_GROUP,
            nam::params::EQ_GROUP,
        ],
        "a NAM block renders the capture tab plus the engine's own tabs"
    );
}

#[test]
fn nam_schema_without_live_axes_has_no_capture_tab() {
    // A NAM whose axes were all dropped as dead (or that declares none)
    // must not grow an empty Capture tab: one group = no tab bar.
    let specs = synthesize_parameters_from_manifest(&nam_package_with_axes());
    assert!(
        specs
            .iter()
            .all(|s| s.group.as_deref() != Some(NAM_CAPTURE_GROUP)),
        "engine-only NAM has no Capture tab; got: {:?}",
        specs
            .iter()
            .map(|s| (&s.path, &s.group))
            .collect::<Vec<_>>()
    );
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
    let specs = synthesize_parameters_from_manifest(&nam_package_with_axes());
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
