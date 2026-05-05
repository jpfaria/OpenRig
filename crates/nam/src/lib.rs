pub mod processor;

use anyhow::{bail, Result};
use processor::{params_from_set, NamPluginParams, NamProcessor};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const GENERIC_NAM_MODEL_ID: &str = "neural_amp_modeler";

pub fn model_schema_for(
    effect_type: &str,
    model: &str,
    display_name: &str,
    include_file_params: bool,
) -> ModelParameterSchema {
    let mut schema = processor::model_schema(include_file_params);
    schema.effect_type = effect_type.to_string();
    schema.model = model.to_string();
    schema.display_name = display_name.to_string();
    schema
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<BlockProcessor> {
    build_processor_for_layout(params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_processor_for_layout(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let (model_path, ir_path, plugin_params) = params_from_set(params)?;
    build_processor_with_assets_for_layout(&model_path, ir_path.as_deref(), plugin_params, sample_rate, layout)
}

pub fn build_processor_with_assets_for_layout(
    model_path: &str,
    ir_path: Option<&str>,
    plugin_params: NamPluginParams,
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(NamProcessor::new(
            model_path,
            ir_path,
            plugin_params,
        )?))),
        AudioChannelLayout::Stereo => {
            bail!("the NAM processor is mono-native and cannot build native stereo processing")
        }
    }
}

// resolve_nam_capture removed in issue #287: its only callers were the
// per-plugin `nam_*.rs` files in crates/block-*/src/, which moved to
// OpenRig-plugins. Plugin-loader resolves capture paths relative to the
// loaded package root.

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::param::ParameterSet;
    use domain::value_objects::ParameterValue;
    use processor::{
        model_schema, plugin_parameter_specs,
        plugin_parameter_specs_with_defaults, plugin_params_from_set,
        plugin_params_from_set_with_defaults, supports_model, NamPluginParams,
        DEFAULT_PLUGIN_PARAMS,
    };

    // ── GENERIC_NAM_MODEL_ID ────────────────────────────────────────

    #[test]
    fn generic_nam_model_id_value_is_expected() {
        assert_eq!(GENERIC_NAM_MODEL_ID, "neural_amp_modeler");
    }

    // ── supports_model ──────────────────────────────────────────────

    #[test]
    fn supports_model_generic_nam_returns_true() {
        assert!(supports_model("neural_amp_modeler"));
    }

    #[test]
    fn supports_model_unknown_returns_false() {
        assert!(!supports_model("some_other_model"));
    }

    #[test]
    fn supports_model_empty_string_returns_false() {
        assert!(!supports_model(""));
    }

    // ── model_schema ────────────────────────────────────────────────

    #[test]
    fn model_schema_with_file_params_includes_model_path() {
        let schema = model_schema(true);
        assert!(schema
            .parameters
            .iter()
            .any(|p| p.path == "model_path"));
    }

    #[test]
    fn model_schema_with_file_params_includes_ir_path() {
        let schema = model_schema(true);
        assert!(schema
            .parameters
            .iter()
            .any(|p| p.path == "ir_path"));
    }

    #[test]
    fn model_schema_without_file_params_excludes_model_path() {
        let schema = model_schema(false);
        assert!(!schema
            .parameters
            .iter()
            .any(|p| p.path == "model_path"));
    }

    #[test]
    fn model_schema_always_includes_plugin_params() {
        let schema = model_schema(false);
        let ids: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(ids.contains(&"input_db"));
        assert!(ids.contains(&"output_db"));
        assert!(ids.contains(&"noise_gate.enabled"));
        assert!(ids.contains(&"noise_gate.threshold_db"));
        assert!(ids.contains(&"eq.enabled"));
        assert!(ids.contains(&"eq.bass"));
        assert!(ids.contains(&"eq.middle"));
        assert!(ids.contains(&"eq.treble"));
    }

    // ── model_schema_for ────────────────────────────────────────────

    #[test]
    fn model_schema_for_sets_effect_type_and_model() {
        let schema = model_schema_for("preamp", "test_model", "Test Model", false);
        assert_eq!(schema.effect_type, "preamp");
        assert_eq!(schema.model, "test_model");
        assert_eq!(schema.display_name, "Test Model");
    }

    // ── plugin_parameter_specs ──────────────────────────────────────

    #[test]
    fn plugin_parameter_specs_returns_eight_params() {
        let specs = plugin_parameter_specs();
        assert_eq!(specs.len(), 8);
    }

    // ── plugin_parameter_specs_with_defaults ─────────────────────────

    #[test]
    fn plugin_parameter_specs_with_defaults_uses_custom_defaults() {
        let custom = NamPluginParams {
            input_level_db: 6.0,
            output_level_db: -3.0,
            noise_gate_threshold_db: -60.0,
            noise_gate_enabled: false,
            eq_enabled: false,
            bass: 3.0,
            middle: 7.0,
            treble: 9.0,
        };
        let specs = plugin_parameter_specs_with_defaults(custom);
        assert_eq!(specs.len(), 8);
    }

    // ── DEFAULT_PLUGIN_PARAMS ───────────────────────────────────────

    #[test]
    fn default_plugin_params_has_expected_values() {
        assert_eq!(DEFAULT_PLUGIN_PARAMS.input_level_db, 0.0);
        assert_eq!(DEFAULT_PLUGIN_PARAMS.output_level_db, 0.0);
        assert_eq!(DEFAULT_PLUGIN_PARAMS.noise_gate_threshold_db, -80.0);
        assert!(DEFAULT_PLUGIN_PARAMS.noise_gate_enabled);
        assert!(DEFAULT_PLUGIN_PARAMS.eq_enabled);
        assert_eq!(DEFAULT_PLUGIN_PARAMS.bass, 5.0);
        assert_eq!(DEFAULT_PLUGIN_PARAMS.middle, 5.0);
        assert_eq!(DEFAULT_PLUGIN_PARAMS.treble, 5.0);
    }

    // ── NamPluginParams struct ──────────────────────────────────────

    #[test]
    fn nam_plugin_params_clone_preserves_values() {
        let params = NamPluginParams {
            input_level_db: 3.0,
            output_level_db: -6.0,
            noise_gate_threshold_db: -40.0,
            noise_gate_enabled: false,
            eq_enabled: true,
            bass: 2.0,
            middle: 8.0,
            treble: 4.0,
        };
        let cloned = params;
        assert_eq!(cloned.input_level_db, 3.0);
        assert_eq!(cloned.output_level_db, -6.0);
        assert!(!cloned.noise_gate_enabled);
    }

    // ── plugin_params_from_set ──────────────────────────────────────

    #[test]
    fn plugin_params_from_set_empty_uses_defaults() {
        let ps = ParameterSet::default();
        let params = plugin_params_from_set(&ps).unwrap();
        assert_eq!(params.input_level_db, DEFAULT_PLUGIN_PARAMS.input_level_db);
        assert_eq!(params.output_level_db, DEFAULT_PLUGIN_PARAMS.output_level_db);
        assert_eq!(params.bass, DEFAULT_PLUGIN_PARAMS.bass);
    }

    #[test]
    fn plugin_params_from_set_overrides_specific_values() {
        let mut ps = ParameterSet::default();
        ps.insert("input_db", ParameterValue::Float(12.0));
        ps.insert("eq.bass", ParameterValue::Float(8.0));
        let params = plugin_params_from_set(&ps).unwrap();
        assert_eq!(params.input_level_db, 12.0);
        assert_eq!(params.bass, 8.0);
        // Others should remain default
        assert_eq!(params.output_level_db, DEFAULT_PLUGIN_PARAMS.output_level_db);
    }

    #[test]
    fn plugin_params_from_set_with_defaults_uses_custom_defaults() {
        let custom = NamPluginParams {
            input_level_db: 3.0,
            output_level_db: -3.0,
            noise_gate_threshold_db: -50.0,
            noise_gate_enabled: false,
            eq_enabled: false,
            bass: 2.0,
            middle: 2.0,
            treble: 2.0,
        };
        let ps = ParameterSet::default();
        let params = plugin_params_from_set_with_defaults(&ps, custom).unwrap();
        assert_eq!(params.input_level_db, 3.0);
        assert!(!params.noise_gate_enabled);
        assert_eq!(params.bass, 2.0);
    }

    // ── params_from_set ─────────────────────────────────────────────

    #[test]
    fn params_from_set_missing_model_path_returns_error() {
        let ps = ParameterSet::default();
        let result = processor::params_from_set(&ps);
        assert!(result.is_err());
    }

    #[test]
    fn params_from_set_with_model_path_succeeds() {
        let mut ps = ParameterSet::default();
        ps.insert("model_path", ParameterValue::String("/path/to/model.nam".into()));
        let (model_path, ir_path, _params) = processor::params_from_set(&ps).unwrap();
        assert_eq!(model_path, "/path/to/model.nam");
        assert!(ir_path.is_none());
    }

    #[test]
    fn params_from_set_with_ir_path_returns_some() {
        let mut ps = ParameterSet::default();
        ps.insert("model_path", ParameterValue::String("/path/model.nam".into()));
        ps.insert("ir_path", ParameterValue::String("/path/ir.wav".into()));
        let (_, ir_path, _) = processor::params_from_set(&ps).unwrap();
        assert_eq!(ir_path, Some("/path/ir.wav".to_string()));
    }

    #[test]
    fn params_from_set_null_ir_path_returns_none() {
        let mut ps = ParameterSet::default();
        ps.insert("model_path", ParameterValue::String("/path/model.nam".into()));
        ps.insert("ir_path", ParameterValue::Null);
        let (_, ir_path, _) = processor::params_from_set(&ps).unwrap();
        assert!(ir_path.is_none());
    }

    // ── build_processor (requires NAM lib) ──────────────────────────

    #[test]
    #[ignore]
    fn build_processor_nonexistent_model_returns_error() {
        let mut ps = ParameterSet::default();
        ps.insert("model_path", ParameterValue::String("/nonexistent.nam".into()));
        let result = build_processor(&ps, 48000.0);
        assert!(result.is_err());
    }

    // ── build_processor_for_layout stereo rejection ─────────────────

    #[test]
    #[ignore]
    fn build_processor_for_layout_stereo_returns_error() {
        let mut ps = ParameterSet::default();
        ps.insert("model_path", ParameterValue::String("/nonexistent.nam".into()));
        let result =
            build_processor_for_layout(&ps, 48000.0, block_core::AudioChannelLayout::Stereo);
        assert!(result.is_err());
    }
}

