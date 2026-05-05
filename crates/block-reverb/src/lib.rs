//! Reverb implementations.
pub mod model_visual;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ReverbBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
    Vst3,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn reverb_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            ReverbBackendKind::Native => "NATIVE",
            ReverbBackendKind::Nam => "NAM",
            ReverbBackendKind::Ir => "IR",
            ReverbBackendKind::Lv2 => "LV2",
            ReverbBackendKind::Vst3 => "VST3",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn reverb_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.display_name).unwrap_or("")
}

pub fn reverb_brand(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.brand).unwrap_or("")
}

pub fn reverb_type_label(model: &str) -> &'static str {
    reverb_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn reverb_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_reverb_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_reverb_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_reverb_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    if let Ok(definition) = registry::find_model_definition(model) {
        return (definition.build)(params, sample_rate, layout);
    }
    if let Some(package) = plugin_loader::registry::find(model) {
        return package.build_processor(params, sample_rate, layout);
    }
    anyhow::bail!("unsupported reverb model '{}'", model)
}


#[cfg(test)]
mod tests {
    use super::{
        build_reverb_processor_for_layout, reverb_brand, reverb_display_name,
        reverb_model_schema, reverb_model_visual, reverb_type_label, supported_models,
    };
    use block_core::param::ParameterSet;
    use domain::value_objects::ParameterValue;
    use block_core::AudioChannelLayout;

    // ── registry-level tests ───────────────────────────────────────────

    #[test]
    fn supported_reverb_models_expose_schema() {
        for model in supported_models() {
            assert!(
                reverb_model_schema(model).is_ok(),
                "expected '{model}' to be supported"
            );
        }
    }

    #[test]
    fn supported_reverb_models_have_nonempty_display_name() {
        for model in supported_models() {
            let name = reverb_display_name(model);
            assert!(!name.is_empty(), "{model} should have a display name");
        }
    }

    #[test]
    fn supported_reverb_models_have_visual_data() {
        for model in supported_models() {
            let visual = reverb_model_visual(model);
            assert!(visual.is_some(), "{model} should have visual data");
        }
    }

    #[test]
    fn supported_reverb_models_have_type_label() {
        for model in supported_models() {
            let label = reverb_type_label(model);
            assert!(!label.is_empty(), "{model} should have a type label");
            assert!(
                ["NATIVE", "LV2", "VST3", "NAM", "IR"].contains(&label),
                "{model} has unexpected type label '{label}'"
            );
        }
    }

    #[test]
    fn unknown_reverb_model_returns_empty_strings() {
        assert_eq!(reverb_display_name("nonexistent_model_xyz"), "");
        assert_eq!(reverb_brand("nonexistent_model_xyz"), "");
        assert_eq!(reverb_type_label("nonexistent_model_xyz"), "");
    }

    #[test]
    fn unknown_reverb_model_schema_fails() {
        assert!(reverb_model_schema("nonexistent_model_xyz").is_err());
    }

    #[test]
    fn unknown_reverb_model_build_fails() {
        let params = ParameterSet::default();
        assert!(build_reverb_processor_for_layout(
            "nonexistent_model_xyz",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .is_err());
    }

    // ── per-native-model tests ─────────────────────────────────────────

    fn native_reverb_models() -> Vec<&'static str> {
        supported_models()
            .iter()
            .copied()
            .filter(|m| reverb_type_label(m) == "NATIVE")
            .collect()
    }

    #[test]
    fn native_reverb_schema_has_mix_param() {
        for model in native_reverb_models() {
            let schema = reverb_model_schema(model).expect("schema");
            assert!(
                schema.parameters.iter().any(|p| p.path == "mix"),
                "{model} schema should contain mix parameter"
            );
        }
    }

    #[test]
    fn native_reverb_validate_accepts_defaults() {
        for model in native_reverb_models() {
            let schema = reverb_model_schema(model).expect("schema");
            let result = ParameterSet::default().normalized_against(&schema);
            assert!(
                result.is_ok(),
                "{model} should accept default parameter values: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn native_reverb_validate_rejects_negative_mix() {
        for model in native_reverb_models() {
            let schema = reverb_model_schema(model).expect("schema");
            let mut ps = ParameterSet::default();
            ps.insert("mix", ParameterValue::Float(-10.0));
            assert!(
                ps.normalized_against(&schema).is_err(),
                "{model} should reject negative mix"
            );
        }
    }

    #[test]
    fn native_reverb_validate_rejects_mix_over_100() {
        for model in native_reverb_models() {
            let schema = reverb_model_schema(model).expect("schema");
            let mut ps = ParameterSet::default();
            ps.insert("mix", ParameterValue::Float(200.0));
            assert!(
                ps.normalized_against(&schema).is_err(),
                "{model} should reject mix=200"
            );
        }
    }

    #[test]
    fn native_reverb_build_mono_with_defaults() {
        for model in native_reverb_models() {
            let schema = reverb_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let result = build_reverb_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Mono,
            );
            assert!(
                result.is_ok(),
                "{model} should build mono processor: {:?}",
                result.err()
            );
        }
    }

    /// Stereo-capable native reverbs: hall, room, spring.
    /// plate_foundation is mono-only and correctly rejects stereo.
    #[test]
    fn native_reverb_stereo_capable_models_build_stereo() {
        let stereo_models = ["hall", "room", "spring"];
        for model in stereo_models {
            let schema = reverb_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let result = build_reverb_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Stereo,
            );
            assert!(
                result.is_ok(),
                "{model} should build stereo processor: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn plate_foundation_rejects_stereo_layout() {
        let schema = reverb_model_schema("plate_foundation").expect("schema");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("normalized defaults");
        let result = build_reverb_processor_for_layout(
            "plate_foundation",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        );
        assert!(
            result.is_err(),
            "plate_foundation should reject stereo layout"
        );
    }

    // ── specific model schema tests ────────────────────────────────────

    #[test]
    fn hall_schema_returns_expected_params() {
        let schema = reverb_model_schema("hall").expect("schema");
        assert_eq!(schema.model, "hall");
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"room_size"));
        assert!(param_names.contains(&"pre_delay_ms"));
        assert!(param_names.contains(&"damping"));
        assert!(param_names.contains(&"mix"));
        assert_eq!(param_names.len(), 4);
    }

    #[test]
    fn room_schema_returns_expected_params() {
        let schema = reverb_model_schema("room").expect("schema");
        assert_eq!(schema.model, "room");
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"room_size"));
        assert!(param_names.contains(&"damping"));
        assert!(param_names.contains(&"mix"));
        assert_eq!(param_names.len(), 3);
    }

    #[test]
    fn plate_foundation_schema_returns_expected_params() {
        let schema = reverb_model_schema("plate_foundation").expect("schema");
        assert_eq!(schema.model, "plate_foundation");
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"room_size"));
        assert!(param_names.contains(&"damping"));
        assert!(param_names.contains(&"mix"));
        assert_eq!(param_names.len(), 3);
    }

    #[test]
    fn spring_schema_returns_expected_params() {
        let schema = reverb_model_schema("spring").expect("schema");
        assert_eq!(schema.model, "spring");
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"tension"));
        assert!(param_names.contains(&"damping"));
        assert!(param_names.contains(&"mix"));
        assert_eq!(param_names.len(), 3);
    }

    #[test]
    fn hall_validate_rejects_room_size_over_100() {
        let schema = reverb_model_schema("hall").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("room_size", ParameterValue::Float(150.0));
        assert!(
            ps.normalized_against(&schema).is_err(),
            "hall should reject room_size=150"
        );
    }

    #[test]
    fn hall_validate_rejects_pre_delay_ms_over_100() {
        let schema = reverb_model_schema("hall").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("pre_delay_ms", ParameterValue::Float(200.0));
        assert!(
            ps.normalized_against(&schema).is_err(),
            "hall should reject pre_delay_ms=200"
        );
    }

    #[test]
    fn room_validate_rejects_damping_over_100() {
        let schema = reverb_model_schema("room").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("damping", ParameterValue::Float(101.0));
        assert!(
            ps.normalized_against(&schema).is_err(),
            "room should reject damping=101"
        );
    }

    #[test]
    fn spring_validate_rejects_tension_over_100() {
        let schema = reverb_model_schema("spring").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("tension", ParameterValue::Float(200.0));
        assert!(
            ps.normalized_against(&schema).is_err(),
            "spring should reject tension=200"
        );
    }

    // ── processing tests (native only) ──────────────────────────────

    #[test]
    fn native_reverb_build_mono_at_44100() {
        for model in native_reverb_models() {
            let schema = reverb_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let result = build_reverb_processor_for_layout(
                model, &params, 44_100.0, AudioChannelLayout::Mono,
            );
            assert!(result.is_ok(), "{model} should build mono at 44100Hz: {:?}", result.err());
        }
    }

    #[test]
    fn native_reverb_stereo_capable_build_stereo_at_44100() {
        let stereo_models = ["hall", "room", "spring"];
        for model in stereo_models {
            let schema = reverb_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let result = build_reverb_processor_for_layout(
                model, &params, 44_100.0, AudioChannelLayout::Stereo,
            );
            assert!(result.is_ok(), "{model} should build stereo at 44100Hz: {:?}", result.err());
        }
    }

    #[test]
    fn native_reverb_process_silence_mono_all_finite() {
        for model in native_reverb_models() {
            let schema = reverb_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let mut processor = build_reverb_processor_for_layout(
                model, &params, 44_100.0, AudioChannelLayout::Mono,
            ).expect("build");

            match &mut processor {
                block_core::BlockProcessor::Mono(ref mut p) => {
                    for i in 0..1024 {
                        let out = p.process_sample(0.0);
                        assert!(out.is_finite(), "{model} mono not finite at sample {i}");
                    }
                }
                _ => panic!("{model} expected Mono processor"),
            }
        }
    }

    #[test]
    fn native_reverb_process_silence_stereo_all_finite() {
        let stereo_models = ["hall", "room", "spring"];
        for model in stereo_models {
            let schema = reverb_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let mut processor = build_reverb_processor_for_layout(
                model, &params, 44_100.0, AudioChannelLayout::Stereo,
            ).expect("build");

            match &mut processor {
                block_core::BlockProcessor::Stereo(ref mut p) => {
                    for i in 0..1024 {
                        let [l, r] = p.process_frame([0.0, 0.0]);
                        assert!(l.is_finite() && r.is_finite(),
                            "{model} stereo not finite at sample {i}");
                    }
                }
                _ => panic!("{model} expected Stereo processor"),
            }
        }
    }

    #[test]
    fn native_reverb_process_sine_mono_all_finite_and_nonzero() {
        for model in native_reverb_models() {
            let schema = reverb_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let mut processor = build_reverb_processor_for_layout(
                model, &params, 44_100.0, AudioChannelLayout::Mono,
            ).expect("build");

            let sr = 44_100.0_f32;
            let mut any_nonzero = false;
            match &mut processor {
                block_core::BlockProcessor::Mono(ref mut p) => {
                    for i in 0..1024 {
                        let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                        let out = p.process_sample(input);
                        assert!(out.is_finite(), "{model} mono not finite at sample {i}");
                        if out.abs() > 1e-10 {
                            any_nonzero = true;
                        }
                    }
                }
                _ => panic!("{model} expected Mono processor"),
            }
            assert!(any_nonzero, "{model} mono produced all zeros for sine input");
        }
    }

    #[test]
    fn native_reverb_process_sine_stereo_all_finite_and_nonzero() {
        let stereo_models = ["hall", "room", "spring"];
        for model in stereo_models {
            let schema = reverb_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let mut processor = build_reverb_processor_for_layout(
                model, &params, 44_100.0, AudioChannelLayout::Stereo,
            ).expect("build");

            let sr = 44_100.0_f32;
            let mut any_nonzero = false;
            match &mut processor {
                block_core::BlockProcessor::Stereo(ref mut p) => {
                    for i in 0..1024 {
                        let input = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                        let [l, r] = p.process_frame([input, input]);
                        assert!(l.is_finite() && r.is_finite(),
                            "{model} stereo not finite at sample {i}");
                        if l.abs() > 1e-10 || r.abs() > 1e-10 {
                            any_nonzero = true;
                        }
                    }
                }
                _ => panic!("{model} expected Stereo processor"),
            }
            assert!(any_nonzero, "{model} stereo produced all zeros for sine input");
        }
    }

    #[test]
    fn native_reverb_process_block_mono_all_finite() {
        for model in native_reverb_models() {
            let schema = reverb_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let mut processor = build_reverb_processor_for_layout(
                model, &params, 44_100.0, AudioChannelLayout::Mono,
            ).expect("build");

            let sr = 44_100.0_f32;
            match &mut processor {
                block_core::BlockProcessor::Mono(ref mut p) => {
                    let mut buffer: Vec<f32> = (0..1024)
                        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
                        .collect();
                    p.process_block(&mut buffer);
                    for (i, s) in buffer.iter().enumerate() {
                        assert!(s.is_finite(), "{model} mono block not finite at frame {i}");
                    }
                }
                _ => panic!("{model} expected Mono processor"),
            }
        }
    }

    #[test]
    fn native_reverb_process_block_stereo_all_finite() {
        let stereo_models = ["hall", "room", "spring"];
        for model in stereo_models {
            let schema = reverb_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let mut processor = build_reverb_processor_for_layout(
                model, &params, 44_100.0, AudioChannelLayout::Stereo,
            ).expect("build");

            let sr = 44_100.0_f32;
            match &mut processor {
                block_core::BlockProcessor::Stereo(ref mut p) => {
                    let mut buffer: Vec<[f32; 2]> = (0..1024)
                        .map(|i| {
                            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
                            [s, s]
                        })
                        .collect();
                    p.process_block(&mut buffer);
                    for (i, [l, r]) in buffer.iter().enumerate() {
                        assert!(l.is_finite() && r.is_finite(),
                            "{model} stereo block not finite at frame {i}");
                    }
                }
                _ => panic!("{model} expected Stereo processor"),
            }
        }
    }
}

/// Push every native model into the unified plugin-loader registry.
/// Called by `adapter-gui` at startup before plugin discovery freezes
/// the catalog.
pub fn register_natives() {
    registry::register_natives();
}
