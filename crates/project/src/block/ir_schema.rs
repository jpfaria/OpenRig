//! IR (`backend: ir`) manifest → parameter schema.

use super::grid_schema::grid_parameter_to_spec;

pub(crate) fn ir_parameters(
    package: &plugin_loader::LoadedPackage,
    parameters: &[plugin_loader::manifest::GridParameter],
    captures: &[plugin_loader::manifest::GridCapture],
) -> Vec<block_core::param::ParameterSpec> {
    // Same dead-axis filter as NAM (issue #649). IR params stay ungrouped:
    // the block editor renders them as one flat grid.
    let axes = plugin_loader::grid_axes::effective_grid_axes(parameters, captures);
    let mut specs: Vec<block_core::param::ParameterSpec> = axes
        .iter()
        .map(|axis| grid_parameter_to_spec(axis, None))
        .collect();
    // Issue #733: a `type: reverb` IR blends dry/wet rather than
    // playing 100% wet at a calibrated level, so it exposes the
    // reverb controls (mix / pre-delay / wet level) in place of the
    // cab-style absolute Output knob.
    if package.manifest.block_type == plugin_loader::manifest::BlockType::Reverb {
        specs.extend(block_reverb::ir_reverb_parameter_specs());
        return specs;
    }
    // Issue #655: user-adjustable Output Level knob (mirrors NAM).
    // The default mirrors the engine baseline — the first capture's
    // audit (manifest-level fallback, 0 dB if neither) — so the knob
    // shows the real applied offset and a fresh block born at the
    // first capture stays unchanged (volume invariant #10). The
    // audio path resolves the offset per-capture from the raw saved
    // params (see `ir::from_package::resolve_output_db`); this
    // default only drives the UI and the new-block seed.
    let default_db = captures
        .first()
        .and_then(|c| c.output_gain_db)
        .or(package.manifest.output_gain_db)
        .unwrap_or(0.0);
    specs.push(block_core::param::float_parameter(
        "output_db",
        "Output",
        None,
        Some(default_db),
        -24.0,
        24.0,
        0.1,
        block_core::param::ParameterUnit::Decibels,
    ));
    specs
}

#[cfg(test)]
mod tests {
    use super::super::dispatch::synthesize_parameters_from_manifest;
    use plugin_loader::manifest::{
        Backend, BlockType, GridCapture, GridParameter, ParameterValue, PluginManifest,
    };
    use plugin_loader::LoadedPackage;
    use std::path::PathBuf;

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

    #[test]
    fn ir_schema_stays_ungrouped() {
        // The Amp/Capture tab split is a NAM affordance (issue #786); an IR
        // block keeps rendering a single flat parameter grid.
        let specs = synthesize_parameters_from_manifest(&ir_package_with_capture_audit(None));
        assert!(
            specs.iter().all(|s| s.group.is_none()),
            "IR params carry no group; got: {:?}",
            specs
                .iter()
                .map(|s| (&s.path, &s.group))
                .collect::<Vec<_>>()
        );
    }
}
