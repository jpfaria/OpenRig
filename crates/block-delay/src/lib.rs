//! Delay implementations.
pub mod model_visual;
mod registry;
pub mod shared;
use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DelayBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
    Vst3,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn delay_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            DelayBackendKind::Native => "NATIVE",
            DelayBackendKind::Nam => "NAM",
            DelayBackendKind::Ir => "IR",
            DelayBackendKind::Lv2 => "LV2",
            DelayBackendKind::Vst3 => "VST3",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
        thumbnail_path: delay_thumbnail(model_id),
        available: registry::is_model_available(model_id),
    })
}

pub fn delay_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.display_name)
        .unwrap_or("")
}

pub fn delay_brand(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.brand)
        .unwrap_or("")
}

pub fn delay_type_label(model: &str) -> &'static str {
    delay_model_visual(model)
        .map(|v| v.type_label)
        .unwrap_or("")
}

pub fn delay_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_delay_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_delay_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_delay_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::{
        build_delay_processor_for_layout, delay_brand, delay_display_name, delay_model_schema,
        delay_model_visual, delay_type_label, supported_models,
    };
    use block_core::param::ParameterSet;
    use block_core::AudioChannelLayout;
    use domain::value_objects::ParameterValue;

    // ── registry-level tests ───────────────────────────────────────────

    #[test]
    fn supported_delay_models_expose_schema() {
        for model in supported_models() {
            assert!(
                delay_model_schema(model).is_ok(),
                "expected '{model}' to be supported"
            );
        }
    }

    #[test]
    fn supported_delay_models_build_for_stereo_chains() {
        // Only test native models; LV2/VST3 require asset_paths initialization.
        for model in native_delay_models() {
            let schema = delay_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let processor = build_delay_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Stereo,
            );

            assert!(processor.is_ok(), "{model} should accept stereo chains");
        }
    }

    #[test]
    fn supported_delay_models_build_for_mono_chains() {
        // Only test native models; LV2/VST3 require asset_paths initialization.
        for model in native_delay_models() {
            let schema = delay_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let processor = build_delay_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Mono,
            );
            assert!(processor.is_ok(), "{model} should accept mono chains");
        }
    }

    #[test]
    fn supported_delay_models_have_nonempty_display_name() {
        for model in supported_models() {
            let name = delay_display_name(model);
            assert!(!name.is_empty(), "{model} should have a display name");
        }
    }

    #[test]
    fn supported_delay_models_have_visual_data() {
        for model in supported_models() {
            let visual = delay_model_visual(model);
            assert!(visual.is_some(), "{model} should have visual data");
        }
    }

    #[test]
    fn supported_delay_models_have_type_label() {
        for model in supported_models() {
            let label = delay_type_label(model);
            assert!(!label.is_empty(), "{model} should have a type label");
            assert!(
                ["NATIVE", "LV2", "VST3", "NAM", "IR"].contains(&label),
                "{model} has unexpected type label '{label}'"
            );
        }
    }

    #[test]
    fn unknown_delay_model_returns_empty_strings() {
        assert_eq!(delay_display_name("nonexistent_model_xyz"), "");
        assert_eq!(delay_brand("nonexistent_model_xyz"), "");
        assert_eq!(delay_type_label("nonexistent_model_xyz"), "");
    }

    #[test]
    fn unknown_delay_model_schema_fails() {
        assert!(delay_model_schema("nonexistent_model_xyz").is_err());
    }

    #[test]
    fn unknown_delay_model_build_fails() {
        let params = ParameterSet::default();
        assert!(build_delay_processor_for_layout(
            "nonexistent_model_xyz",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .is_err());
    }

    // ── per-native-model schema/validate/build tests ───────────────────

    fn native_delay_models() -> Vec<&'static str> {
        supported_models()
            .iter()
            .copied()
            .filter(|m| delay_type_label(m) == "NATIVE")
            .collect()
    }

    #[test]
    fn native_delay_schema_has_time_ms_param() {
        for model in native_delay_models() {
            let schema = delay_model_schema(model).expect("schema");
            assert!(
                schema.parameters.iter().any(|p| p.path == "time_ms"),
                "{model} schema should contain time_ms parameter"
            );
        }
    }

    #[test]
    fn native_delay_schema_has_mix_param() {
        for model in native_delay_models() {
            let schema = delay_model_schema(model).expect("schema");
            assert!(
                schema.parameters.iter().any(|p| p.path == "mix"),
                "{model} schema should contain mix parameter"
            );
        }
    }

    #[test]
    fn native_delay_schema_has_feedback_param() {
        for model in native_delay_models() {
            let schema = delay_model_schema(model).expect("schema");
            assert!(
                schema.parameters.iter().any(|p| p.path == "feedback"),
                "{model} schema should contain feedback parameter"
            );
        }
    }

    #[test]
    fn native_delay_validate_accepts_defaults() {
        for model in native_delay_models() {
            let schema = delay_model_schema(model).expect("schema");
            let result = ParameterSet::default().normalized_against(&schema);
            assert!(
                result.is_ok(),
                "{model} should accept default parameter values: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn native_delay_validate_rejects_out_of_range_time_ms() {
        for model in native_delay_models() {
            let schema = delay_model_schema(model).expect("schema");
            let mut ps = ParameterSet::default();
            ps.insert("time_ms", ParameterValue::Float(5000.0));
            assert!(
                ps.normalized_against(&schema).is_err(),
                "{model} should reject time_ms=5000 (out of range)"
            );
        }
    }

    #[test]
    fn native_delay_validate_rejects_negative_mix() {
        for model in native_delay_models() {
            let schema = delay_model_schema(model).expect("schema");
            let mut ps = ParameterSet::default();
            ps.insert("mix", ParameterValue::Float(-10.0));
            assert!(
                ps.normalized_against(&schema).is_err(),
                "{model} should reject negative mix"
            );
        }
    }

    #[test]
    fn native_delay_validate_rejects_feedback_over_100() {
        for model in native_delay_models() {
            let schema = delay_model_schema(model).expect("schema");
            let mut ps = ParameterSet::default();
            ps.insert("feedback", ParameterValue::Float(150.0));
            assert!(
                ps.normalized_against(&schema).is_err(),
                "{model} should reject feedback=150 (over max)"
            );
        }
    }

    // ── specific native model tests ────────────────────────────────────

    #[test]
    fn digital_clean_schema_returns_expected_params() {
        let schema = delay_model_schema("digital_clean").expect("schema");
        assert_eq!(schema.model, "digital_clean");
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"time_ms"));
        assert!(param_names.contains(&"feedback"));
        assert!(param_names.contains(&"mix"));
        assert_eq!(param_names.len(), 3);
    }

    #[test]
    fn analog_warm_schema_returns_expected_params() {
        let schema = delay_model_schema("analog_warm").expect("schema");
        assert_eq!(schema.model, "analog_warm");
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"time_ms"));
        assert!(param_names.contains(&"feedback"));
        assert!(param_names.contains(&"mix"));
        assert!(param_names.contains(&"tone"));
        assert_eq!(param_names.len(), 4);
    }

    #[test]
    fn slapback_schema_returns_expected_params() {
        let schema = delay_model_schema("slapback").expect("schema");
        assert_eq!(schema.model, "slapback");
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(param_names.len(), 3);
    }

    #[test]
    fn reverse_schema_returns_expected_params() {
        let schema = delay_model_schema("reverse").expect("schema");
        assert_eq!(schema.model, "reverse");
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(param_names.len(), 3);
    }

    #[test]
    fn modulated_delay_schema_returns_expected_params() {
        let schema = delay_model_schema("modulated_delay").expect("schema");
        assert_eq!(schema.model, "modulated_delay");
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"time_ms"));
        assert!(param_names.contains(&"feedback"));
        assert!(param_names.contains(&"mix"));
        assert!(param_names.contains(&"rate_hz"));
        assert!(param_names.contains(&"depth"));
        assert_eq!(param_names.len(), 5);
    }

    #[test]
    fn tape_vintage_schema_returns_expected_params() {
        let schema = delay_model_schema("tape_vintage").expect("schema");
        assert_eq!(schema.model, "tape_vintage");
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"time_ms"));
        assert!(param_names.contains(&"feedback"));
        assert!(param_names.contains(&"mix"));
        assert!(param_names.contains(&"tone"));
        assert!(param_names.contains(&"flutter"));
        assert_eq!(param_names.len(), 5);
    }

    #[test]
    fn analog_warm_validate_rejects_tone_over_100() {
        let schema = delay_model_schema("analog_warm").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("tone", ParameterValue::Float(200.0));
        assert!(
            ps.normalized_against(&schema).is_err(),
            "analog_warm should reject tone=200"
        );
    }

    #[test]
    fn modulated_delay_validate_rejects_rate_hz_over_max() {
        let schema = delay_model_schema("modulated_delay").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("rate_hz", ParameterValue::Float(20.0));
        assert!(
            ps.normalized_against(&schema).is_err(),
            "modulated_delay should reject rate_hz=20 (max is 8)"
        );
    }

    #[test]
    fn tape_vintage_validate_rejects_flutter_over_100() {
        let schema = delay_model_schema("tape_vintage").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("flutter", ParameterValue::Float(200.0));
        assert!(
            ps.normalized_against(&schema).is_err(),
            "tape_vintage should reject flutter=200"
        );
    }

    // ── processing tests (native only) ──────────────────────────────────

    fn default_params_for(model: &str) -> ParameterSet {
        let schema = delay_model_schema(model).expect("schema");
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("normalized defaults")
    }

    #[test]
    fn native_delay_process_silence_mono_produces_finite() {
        for model in native_delay_models() {
            let params = default_params_for(model);
            let mut processor =
                build_delay_processor_for_layout(model, &params, 44100.0, AudioChannelLayout::Mono)
                    .expect("build");

            match &mut processor {
                block_core::BlockProcessor::Mono(ref mut p) => {
                    for i in 0..1024 {
                        let out = p.process_sample(0.0);
                        assert!(
                            out.is_finite(),
                            "{model} mono produced non-finite at sample {i}: {out}"
                        );
                    }
                }
                _ => panic!("{model} expected Mono processor"),
            }
        }
    }

    #[test]
    fn native_delay_process_sine_mono_produces_finite() {
        for model in native_delay_models() {
            let params = default_params_for(model);
            let mut processor =
                build_delay_processor_for_layout(model, &params, 44100.0, AudioChannelLayout::Mono)
                    .expect("build");

            match &mut processor {
                block_core::BlockProcessor::Mono(ref mut p) => {
                    for i in 0..1024 {
                        let input =
                            (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                        let out = p.process_sample(input);
                        assert!(
                            out.is_finite(),
                            "{model} mono produced non-finite at sample {i}: {out}"
                        );
                    }
                }
                _ => panic!("{model} expected Mono processor"),
            }
        }
    }

    #[test]
    fn native_delay_process_silence_stereo_produces_finite() {
        for model in native_delay_models() {
            let params = default_params_for(model);
            let mut processor = build_delay_processor_for_layout(
                model,
                &params,
                44100.0,
                AudioChannelLayout::Stereo,
            )
            .expect("build");

            match &mut processor {
                block_core::BlockProcessor::Stereo(ref mut p) => {
                    for i in 0..1024 {
                        let [l, r] = p.process_frame([0.0, 0.0]);
                        assert!(
                            l.is_finite() && r.is_finite(),
                            "{model} stereo produced non-finite at frame {i}: [{l}, {r}]"
                        );
                    }
                }
                _ => panic!("{model} expected Stereo processor"),
            }
        }
    }

    #[test]
    fn native_delay_process_sine_stereo_produces_finite() {
        for model in native_delay_models() {
            let params = default_params_for(model);
            let mut processor = build_delay_processor_for_layout(
                model,
                &params,
                44100.0,
                AudioChannelLayout::Stereo,
            )
            .expect("build");

            match &mut processor {
                block_core::BlockProcessor::Stereo(ref mut p) => {
                    for i in 0..1024 {
                        let s = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                        let [l, r] = p.process_frame([s, s]);
                        assert!(
                            l.is_finite() && r.is_finite(),
                            "{model} stereo produced non-finite at frame {i}: [{l}, {r}]"
                        );
                    }
                }
                _ => panic!("{model} expected Stereo processor"),
            }
        }
    }

    #[test]
    fn native_delay_process_block_1024_silence_mono_all_finite() {
        for model in native_delay_models() {
            let params = default_params_for(model);
            let mut processor =
                build_delay_processor_for_layout(model, &params, 44100.0, AudioChannelLayout::Mono)
                    .expect("build");

            match &mut processor {
                block_core::BlockProcessor::Mono(ref mut p) => {
                    let mut buf = vec![0.0_f32; 1024];
                    p.process_block(&mut buf);
                    for (i, &s) in buf.iter().enumerate() {
                        assert!(
                            s.is_finite(),
                            "{model} mono block silence non-finite at {i}: {s}"
                        );
                    }
                }
                _ => panic!("{model} expected Mono processor"),
            }
        }
    }

    #[test]
    fn native_delay_process_block_1024_sine_mono_all_finite() {
        for model in native_delay_models() {
            let params = default_params_for(model);
            let mut processor =
                build_delay_processor_for_layout(model, &params, 44100.0, AudioChannelLayout::Mono)
                    .expect("build");

            match &mut processor {
                block_core::BlockProcessor::Mono(ref mut p) => {
                    let mut buf: Vec<f32> = (0..1024)
                        .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
                        .collect();
                    p.process_block(&mut buf);
                    for (i, &s) in buf.iter().enumerate() {
                        assert!(
                            s.is_finite(),
                            "{model} mono block sine non-finite at {i}: {s}"
                        );
                    }
                }
                _ => panic!("{model} expected Mono processor"),
            }
        }
    }

    #[test]
    fn native_delay_process_block_1024_sine_stereo_all_finite() {
        for model in native_delay_models() {
            let params = default_params_for(model);
            let mut processor = build_delay_processor_for_layout(
                model,
                &params,
                44100.0,
                AudioChannelLayout::Stereo,
            )
            .expect("build");

            match &mut processor {
                block_core::BlockProcessor::Stereo(ref mut p) => {
                    let mut buf: Vec<[f32; 2]> = (0..1024)
                        .map(|i| {
                            let s =
                                (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                            [s, s]
                        })
                        .collect();
                    p.process_block(&mut buf);
                    for (i, &[l, r]) in buf.iter().enumerate() {
                        assert!(
                            l.is_finite() && r.is_finite(),
                            "{model} stereo block sine non-finite at {i}: [{l}, {r}]"
                        );
                    }
                }
                _ => panic!("{model} expected Stereo processor"),
            }
        }
    }
}

pub fn is_delay_model_available(model: &str) -> bool {
    registry::is_model_available(model)
}

/// Returns the catalog thumbnail path (relative to project root) for a model,
/// or `None` if the model has no thumbnail registered.
pub fn delay_thumbnail(model: &str) -> Option<&'static str> {
    registry::THUMBNAILS
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, path)| *path)
}
