use crate::ModelAudioMode;
use domain::ids::{BlockId, ParameterId};
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ParameterSet {
    pub values: BTreeMap<String, ParameterValue>,
}

impl ParameterSet {
    pub fn insert(&mut self, path: impl Into<String>, value: ParameterValue) {
        self.values.insert(path.into(), value);
    }

    pub fn get(&self, path: &str) -> Option<&ParameterValue> {
        self.values.get(path)
    }

    pub fn get_bool(&self, path: &str) -> Option<bool> {
        self.get(path).and_then(ParameterValue::as_bool)
    }

    pub fn get_i64(&self, path: &str) -> Option<i64> {
        self.get(path).and_then(ParameterValue::as_i64)
    }

    pub fn get_f32(&self, path: &str) -> Option<f32> {
        self.get(path).and_then(ParameterValue::as_f32)
    }

    pub fn get_string(&self, path: &str) -> Option<&str> {
        self.get(path).and_then(ParameterValue::as_str)
    }

    pub fn get_optional_string(&self, path: &str) -> Option<Option<&str>> {
        self.get(path).map(|value| {
            if value.is_null() {
                None
            } else {
                value.as_str()
            }
        })
    }

    pub fn normalized_against(&self, schema: &ModelParameterSchema) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        let mut known_specs = BTreeMap::new();
        for spec in &schema.parameters {
            known_specs.insert(spec.path.as_str(), spec);
        }

        for (path, value) in &self.values {
            let Some(spec) = known_specs.get(path.as_str()) else {
                // Keep unknown parameters instead of silently dropping them.
                // They may belong to a different version of the model or be
                // internal state that should survive round-trips.
                log::warn!(
                    "[param] keeping unknown parameter '{}' (not in schema for {} model '{}')",
                    path, schema.effect_type, schema.model
                );
                values.insert(path.clone(), value.clone());
                continue;
            };
            spec.validate_value(value).map_err(|error| {
                format!(
                    "invalid parameter '{}' for {} model '{}': {}",
                    path, schema.effect_type, schema.model, error
                )
            })?;
            values.insert(path.clone(), value.clone());
        }

        for spec in &schema.parameters {
            match values.get(&spec.path) {
                Some(value) => {
                    spec.validate_value(value).map_err(|error| {
                        format!(
                            "invalid parameter '{}' for {} model '{}': {}",
                            spec.path, schema.effect_type, schema.model, error
                        )
                    })?;
                }
                None => match &spec.default_value {
                    Some(default_value) => {
                        values.insert(spec.path.clone(), default_value.clone());
                    }
                    None => {
                        return Err(format!(
                            "missing required parameter '{}' for {} model '{}'",
                            spec.path, schema.effect_type, schema.model
                        ));
                    }
                },
            }
        }

        Ok(Self { values })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelParameterSchema {
    pub effect_type: String,
    pub model: String,
    pub display_name: String,
    pub audio_mode: ModelAudioMode,
    pub parameters: Vec<ParameterSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterSpec {
    pub path: String,
    pub label: String,
    pub group: Option<String>,
    pub widget: ParameterWidget,
    pub unit: ParameterUnit,
    pub domain: ParameterDomain,
    pub default_value: Option<ParameterValue>,
    pub optional: bool,
    pub allow_empty: bool,
}

impl ParameterSpec {
    pub fn validate_value(&self, value: &ParameterValue) -> Result<(), String> {
        if value.is_null() {
            if self.optional {
                return Ok(());
            }
            return Err("null is not allowed".to_string());
        }

        match (&self.domain, value) {
            (ParameterDomain::Bool, ParameterValue::Bool(_)) => Ok(()),
            (ParameterDomain::IntRange { min, max, step }, ParameterValue::Int(actual)) => {
                validate_int_range(*actual, *min, *max, *step)
            }
            (ParameterDomain::FloatRange { min, max, step }, ParameterValue::Float(actual)) => {
                validate_float_range(*actual, *min, *max, *step)
            }
            (ParameterDomain::FloatRange { min, max, step }, ParameterValue::Int(actual)) => {
                validate_float_range(*actual as f32, *min, *max, *step)
            }
            (ParameterDomain::Enum { options }, ParameterValue::String(actual)) => {
                if options.iter().any(|option| option.value == *actual) {
                    Ok(())
                } else {
                    Err(format!("'{}' is not an allowed option", actual))
                }
            }
            (ParameterDomain::Text, ParameterValue::String(actual)) => {
                validate_text(actual, self.allow_empty)
            }
            (ParameterDomain::FilePath { extensions }, ParameterValue::String(actual)) => {
                validate_text(actual, self.allow_empty)?;
                validate_file_path(actual, extensions)
            }
            _ => Err(format!(
                "expected {:?}, got {:?}",
                self.domain.value_kind(),
                value
            )),
        }
    }

    pub fn materialize(
        &self,
        block_id: &BlockId,
        effect_type: &str,
        model: &str,
        audio_mode: ModelAudioMode,
        current_value: ParameterValue,
    ) -> BlockParameterDescriptor {
        BlockParameterDescriptor {
            id: ParameterId::for_block_path(block_id, &self.path),
            block_id: block_id.clone(),
            effect_type: effect_type.to_string(),
            model: model.to_string(),
            audio_mode,
            path: self.path.clone(),
            label: self.label.clone(),
            group: self.group.clone(),
            widget: self.widget.clone(),
            unit: self.unit.clone(),
            domain: self.domain.clone(),
            default_value: self.default_value.clone(),
            current_value,
            optional: self.optional,
            allow_empty: self.allow_empty,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockParameterDescriptor {
    pub id: ParameterId,
    pub block_id: BlockId,
    pub effect_type: String,
    pub model: String,
    pub audio_mode: ModelAudioMode,
    pub path: String,
    pub label: String,
    pub group: Option<String>,
    pub widget: ParameterWidget,
    pub unit: ParameterUnit,
    pub domain: ParameterDomain,
    pub default_value: Option<ParameterValue>,
    pub current_value: ParameterValue,
    pub optional: bool,
    pub allow_empty: bool,
}

impl BlockParameterDescriptor {
    pub fn validate_value(&self, value: &ParameterValue) -> Result<(), String> {
        ParameterSpec {
            path: self.path.clone(),
            label: self.label.clone(),
            group: self.group.clone(),
            widget: self.widget.clone(),
            unit: self.unit.clone(),
            domain: self.domain.clone(),
            default_value: self.default_value.clone(),
            optional: self.optional,
            allow_empty: self.allow_empty,
        }
        .validate_value(value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterWidget {
    Knob,
    Toggle,
    Select,
    FilePicker,
    TextInput,
    MultiSlider,
    CurveEditor { role: CurveEditorRole },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CurveEditorRole {
    X,
    Y,
    Width,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterUnit {
    None,
    Decibels,
    Hertz,
    Milliseconds,
    Percent,
    Ratio,
    Semitones,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterDomain {
    Bool,
    IntRange { min: i64, max: i64, step: i64 },
    FloatRange { min: f32, max: f32, step: f32 },
    Enum { options: Vec<ParameterOption> },
    Text,
    FilePath { extensions: Vec<String> },
}

impl ParameterDomain {
    fn value_kind(&self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::IntRange { .. } => "int",
            Self::FloatRange { .. } => "float",
            Self::Enum { .. } => "enum",
            Self::Text => "string",
            Self::FilePath { .. } => "path",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterOption {
    pub value: String,
    pub label: String,
}

#[allow(clippy::too_many_arguments)]
pub fn float_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<f32>,
    min: f32,
    max: f32,
    step: f32,
    unit: ParameterUnit,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::Knob,
        unit,
        domain: ParameterDomain::FloatRange { min, max, step },
        default_value: default_value.map(ParameterValue::Float),
        optional: false,
        allow_empty: false,
    }
}

pub fn bool_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<bool>,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::Toggle,
        unit: ParameterUnit::None,
        domain: ParameterDomain::Bool,
        default_value: default_value.map(ParameterValue::Bool),
        optional: false,
        allow_empty: false,
    }
}

pub fn file_path_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<ParameterValue>,
    extensions: &[&str],
    optional: bool,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::FilePicker,
        unit: ParameterUnit::None,
        domain: ParameterDomain::FilePath {
            extensions: extensions
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        },
        default_value,
        optional,
        allow_empty: false,
    }
}

pub fn enum_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<&str>,
    options: &[(&str, &str)],
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::Select,
        unit: ParameterUnit::None,
        domain: ParameterDomain::Enum {
            options: options
                .iter()
                .map(|(value, option_label)| ParameterOption {
                    value: (*value).to_string(),
                    label: (*option_label).to_string(),
                })
                .collect(),
        },
        default_value: default_value.map(|value| ParameterValue::String(value.to_string())),
        optional: false,
        allow_empty: false,
    }
}

pub fn text_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<&str>,
    optional: bool,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::TextInput,
        unit: ParameterUnit::None,
        domain: ParameterDomain::Text,
        default_value: default_value.map(|value| ParameterValue::String(value.to_string())),
        optional,
        allow_empty: false,
    }
}

pub fn multi_slider_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    default_value: Option<f32>,
    min: f32,
    max: f32,
    step: f32,
    unit: ParameterUnit,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::MultiSlider,
        unit,
        domain: ParameterDomain::FloatRange { min, max, step },
        default_value: default_value.map(ParameterValue::Float),
        optional: false,
        allow_empty: false,
    }
}

pub fn curve_editor_parameter(
    path: &str,
    label: &str,
    group: Option<&str>,
    role: CurveEditorRole,
    default_value: Option<f32>,
    min: f32,
    max: f32,
    step: f32,
    unit: ParameterUnit,
) -> ParameterSpec {
    ParameterSpec {
        path: path.to_string(),
        label: label.to_string(),
        group: group.map(ToString::to_string),
        widget: ParameterWidget::CurveEditor { role },
        unit,
        domain: ParameterDomain::FloatRange { min, max, step },
        default_value: default_value.map(ParameterValue::Float),
        optional: false,
        allow_empty: false,
    }
}

pub fn required_f32(params: &ParameterSet, path: &str) -> Result<f32, String> {
    params
        .get_f32(path)
        .ok_or_else(|| format!("missing or invalid float parameter '{}'", path))
}

pub fn required_bool(params: &ParameterSet, path: &str) -> Result<bool, String> {
    params
        .get_bool(path)
        .ok_or_else(|| format!("missing or invalid bool parameter '{}'", path))
}

pub fn required_string(params: &ParameterSet, path: &str) -> Result<String, String> {
    params
        .get_string(path)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing or invalid string parameter '{}'", path))
}

pub fn optional_string(params: &ParameterSet, path: &str) -> Option<String> {
    params
        .get_optional_string(path)
        .flatten()
        .map(ToString::to_string)
}

fn validate_text(value: &str, allow_empty: bool) -> Result<(), String> {
    if !allow_empty && value.trim().is_empty() {
        Err("value cannot be empty".to_string())
    } else {
        Ok(())
    }
}

fn validate_file_path(value: &str, extensions: &[String]) -> Result<(), String> {
    if extensions.is_empty() {
        return Ok(());
    }
    let lower = value.to_ascii_lowercase();
    if extensions
        .iter()
        .any(|extension| lower.ends_with(&format!(".{}", extension.to_ascii_lowercase())))
    {
        Ok(())
    } else {
        Err(format!(
            "path '{}' must end with one of: {}",
            value,
            extensions.join(", ")
        ))
    }
}

fn validate_int_range(value: i64, min: i64, max: i64, step: i64) -> Result<(), String> {
    if value < min || value > max {
        return Err(format!(
            "value {} is outside range [{}..={}]",
            value, min, max
        ));
    }
    if step > 0 && (value - min) % step != 0 {
        return Err(format!("value {} does not align with step {}", value, step));
    }
    Ok(())
}

fn validate_float_range(value: f32, min: f32, max: f32, step: f32) -> Result<(), String> {
    if value < min || value > max {
        return Err(format!(
            "value {} is outside range [{}..={}]",
            value, min, max
        ));
    }
    if step > 0.0 {
        let steps = (value - min) / step;
        let nearest = steps.round();
        if (steps - nearest).abs() > 1e-4 {
            return Err(format!("value {} does not align with step {}", value, step));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper: build a simple schema ───────────────────────────────

    fn test_schema(params: Vec<ParameterSpec>) -> ModelParameterSchema {
        ModelParameterSchema {
            effect_type: "test".to_string(),
            model: "test_model".to_string(),
            display_name: "Test Model".to_string(),
            audio_mode: ModelAudioMode::MonoOnly,
            parameters: params,
        }
    }

    // ── ParameterSet basics ─────────────────────────────────────────

    #[test]
    fn parameter_set_insert_and_get() {
        let mut ps = ParameterSet::default();
        ps.insert("gain", ParameterValue::Float(50.0));
        assert_eq!(ps.get("gain"), Some(&ParameterValue::Float(50.0)));
        assert_eq!(ps.get("missing"), None);
    }

    #[test]
    fn parameter_set_get_bool_success() {
        let mut ps = ParameterSet::default();
        ps.insert("bright", ParameterValue::Bool(true));
        assert_eq!(ps.get_bool("bright"), Some(true));
    }

    #[test]
    fn parameter_set_get_bool_wrong_type_returns_none() {
        let mut ps = ParameterSet::default();
        ps.insert("bright", ParameterValue::Float(1.0));
        assert_eq!(ps.get_bool("bright"), None);
    }

    #[test]
    fn parameter_set_get_i64_success() {
        let mut ps = ParameterSet::default();
        ps.insert("count", ParameterValue::Int(42));
        assert_eq!(ps.get_i64("count"), Some(42));
    }

    #[test]
    fn parameter_set_get_f32_from_float() {
        let mut ps = ParameterSet::default();
        ps.insert("gain", ParameterValue::Float(3.14));
        assert!((ps.get_f32("gain").unwrap() - 3.14).abs() < 1e-4);
    }

    #[test]
    fn parameter_set_get_f32_from_int() {
        let mut ps = ParameterSet::default();
        ps.insert("gain", ParameterValue::Int(10));
        assert_eq!(ps.get_f32("gain"), Some(10.0));
    }

    #[test]
    fn parameter_set_get_string_success() {
        let mut ps = ParameterSet::default();
        ps.insert("key", ParameterValue::String("C".to_string()));
        assert_eq!(ps.get_string("key"), Some("C"));
    }

    #[test]
    fn parameter_set_get_string_wrong_type() {
        let mut ps = ParameterSet::default();
        ps.insert("key", ParameterValue::Bool(false));
        assert_eq!(ps.get_string("key"), None);
    }

    #[test]
    fn parameter_set_get_optional_string_present() {
        let mut ps = ParameterSet::default();
        ps.insert("path", ParameterValue::String("foo.wav".to_string()));
        assert_eq!(ps.get_optional_string("path"), Some(Some("foo.wav")));
    }

    #[test]
    fn parameter_set_get_optional_string_null() {
        let mut ps = ParameterSet::default();
        ps.insert("path", ParameterValue::Null);
        assert_eq!(ps.get_optional_string("path"), Some(None));
    }

    #[test]
    fn parameter_set_get_optional_string_missing() {
        let ps = ParameterSet::default();
        assert_eq!(ps.get_optional_string("path"), None);
    }

    // ── ParameterSpec::validate_value — Float domain ────────────────

    #[test]
    fn validate_float_in_range_ok() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 1.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Float(50.0)).is_ok());
    }

    #[test]
    fn validate_float_at_min_boundary() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 1.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Float(0.0)).is_ok());
    }

    #[test]
    fn validate_float_at_max_boundary() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 1.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Float(100.0)).is_ok());
    }

    #[test]
    fn validate_float_below_min_fails() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 1.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Float(-1.0)).is_err());
    }

    #[test]
    fn validate_float_above_max_fails() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 1.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Float(101.0)).is_err());
    }

    #[test]
    fn validate_float_step_alignment_ok() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 10.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Float(30.0)).is_ok());
    }

    #[test]
    fn validate_float_step_misalignment_fails() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 10.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Float(35.0)).is_err());
    }

    #[test]
    fn validate_float_zero_step_allows_any_value() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 0.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Float(33.333)).is_ok());
    }

    #[test]
    fn validate_float_accepts_int_value() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 0.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Int(50)).is_ok());
    }

    #[test]
    fn validate_float_rejects_bool() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 1.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Bool(true)).is_err());
    }

    // ── ParameterSpec::validate_value — Bool domain ─────────────────

    #[test]
    fn validate_bool_true_ok() {
        let spec = bool_parameter("bright", "Bright", None, Some(false));
        assert!(spec.validate_value(&ParameterValue::Bool(true)).is_ok());
    }

    #[test]
    fn validate_bool_false_ok() {
        let spec = bool_parameter("bright", "Bright", None, Some(true));
        assert!(spec.validate_value(&ParameterValue::Bool(false)).is_ok());
    }

    #[test]
    fn validate_bool_rejects_float() {
        let spec = bool_parameter("bright", "Bright", None, None);
        assert!(spec.validate_value(&ParameterValue::Float(1.0)).is_err());
    }

    // ── ParameterSpec::validate_value — Enum domain ─────────────────

    #[test]
    fn validate_enum_valid_option_ok() {
        let spec = enum_parameter("key", "Key", None, Some("C"), &[("C", "C Major"), ("D", "D Major")]);
        assert!(spec.validate_value(&ParameterValue::String("C".to_string())).is_ok());
    }

    #[test]
    fn validate_enum_invalid_option_fails() {
        let spec = enum_parameter("key", "Key", None, Some("C"), &[("C", "C Major"), ("D", "D Major")]);
        assert!(spec.validate_value(&ParameterValue::String("E".to_string())).is_err());
    }

    #[test]
    fn validate_enum_rejects_non_string() {
        let spec = enum_parameter("key", "Key", None, Some("C"), &[("C", "C Major")]);
        assert!(spec.validate_value(&ParameterValue::Int(0)).is_err());
    }

    // ── ParameterSpec::validate_value — Text domain ─────────────────

    #[test]
    fn validate_text_non_empty_ok() {
        let spec = text_parameter("name", "Name", None, None, false);
        assert!(spec.validate_value(&ParameterValue::String("hello".to_string())).is_ok());
    }

    #[test]
    fn validate_text_empty_fails_when_not_allowed() {
        let spec = text_parameter("name", "Name", None, None, false);
        assert!(spec.validate_value(&ParameterValue::String("".to_string())).is_err());
    }

    #[test]
    fn validate_text_whitespace_only_fails_when_not_allowed() {
        let spec = text_parameter("name", "Name", None, None, false);
        assert!(spec.validate_value(&ParameterValue::String("   ".to_string())).is_err());
    }

    // ── ParameterSpec::validate_value — FilePath domain ──────────────

    #[test]
    fn validate_file_path_valid_extension_ok() {
        let spec = file_path_parameter("file", "File", None, None, &["wav", "mp3"], false);
        assert!(spec.validate_value(&ParameterValue::String("/path/to/file.wav".to_string())).is_ok());
    }

    #[test]
    fn validate_file_path_wrong_extension_fails() {
        let spec = file_path_parameter("file", "File", None, None, &["wav", "mp3"], false);
        assert!(spec.validate_value(&ParameterValue::String("/path/to/file.txt".to_string())).is_err());
    }

    #[test]
    fn validate_file_path_case_insensitive_extension() {
        let spec = file_path_parameter("file", "File", None, None, &["wav"], false);
        assert!(spec.validate_value(&ParameterValue::String("file.WAV".to_string())).is_ok());
    }

    #[test]
    fn validate_file_path_no_extensions_allows_any() {
        let spec = file_path_parameter("file", "File", None, None, &[], false);
        assert!(spec.validate_value(&ParameterValue::String("anything.xyz".to_string())).is_ok());
    }

    #[test]
    fn validate_file_path_empty_string_fails() {
        let spec = file_path_parameter("file", "File", None, None, &["wav"], false);
        assert!(spec.validate_value(&ParameterValue::String("".to_string())).is_err());
    }

    // ── ParameterSpec::validate_value — Null handling ────────────────

    #[test]
    fn validate_null_on_required_fails() {
        let spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 1.0, ParameterUnit::Percent);
        assert!(spec.validate_value(&ParameterValue::Null).is_err());
    }

    #[test]
    fn validate_null_on_optional_ok() {
        let mut spec = float_parameter("gain", "Gain", None, None, 0.0, 100.0, 1.0, ParameterUnit::Percent);
        spec.optional = true;
        assert!(spec.validate_value(&ParameterValue::Null).is_ok());
    }

    // ── ParameterSpec::validate_value — IntRange domain ──────────────

    #[test]
    fn validate_int_range_in_range_ok() {
        let spec = ParameterSpec {
            path: "count".to_string(),
            label: "Count".to_string(),
            group: None,
            widget: ParameterWidget::Knob,
            unit: ParameterUnit::None,
            domain: ParameterDomain::IntRange { min: 0, max: 10, step: 2 },
            default_value: None,
            optional: false,
            allow_empty: false,
        };
        assert!(spec.validate_value(&ParameterValue::Int(4)).is_ok());
    }

    #[test]
    fn validate_int_range_out_of_range_fails() {
        let spec = ParameterSpec {
            path: "count".to_string(),
            label: "Count".to_string(),
            group: None,
            widget: ParameterWidget::Knob,
            unit: ParameterUnit::None,
            domain: ParameterDomain::IntRange { min: 0, max: 10, step: 1 },
            default_value: None,
            optional: false,
            allow_empty: false,
        };
        assert!(spec.validate_value(&ParameterValue::Int(11)).is_err());
    }

    #[test]
    fn validate_int_range_step_misalignment_fails() {
        let spec = ParameterSpec {
            path: "count".to_string(),
            label: "Count".to_string(),
            group: None,
            widget: ParameterWidget::Knob,
            unit: ParameterUnit::None,
            domain: ParameterDomain::IntRange { min: 0, max: 10, step: 3 },
            default_value: None,
            optional: false,
            allow_empty: false,
        };
        assert!(spec.validate_value(&ParameterValue::Int(4)).is_err());
        assert!(spec.validate_value(&ParameterValue::Int(6)).is_ok());
    }

    #[test]
    fn validate_int_range_zero_step_allows_any() {
        let spec = ParameterSpec {
            path: "count".to_string(),
            label: "Count".to_string(),
            group: None,
            widget: ParameterWidget::Knob,
            unit: ParameterUnit::None,
            domain: ParameterDomain::IntRange { min: 0, max: 10, step: 0 },
            default_value: None,
            optional: false,
            allow_empty: false,
        };
        assert!(spec.validate_value(&ParameterValue::Int(7)).is_ok());
    }

    // ── normalized_against ──────────────────────────────────────────

    #[test]
    fn normalized_against_fills_defaults() {
        let schema = test_schema(vec![
            float_parameter("gain", "Gain", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ]);
        let ps = ParameterSet::default();
        let result = ps.normalized_against(&schema).unwrap();
        assert_eq!(result.get_f32("gain"), Some(50.0));
    }

    #[test]
    fn normalized_against_keeps_existing_values() {
        let schema = test_schema(vec![
            float_parameter("gain", "Gain", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ]);
        let mut ps = ParameterSet::default();
        ps.insert("gain", ParameterValue::Float(75.0));
        let result = ps.normalized_against(&schema).unwrap();
        assert_eq!(result.get_f32("gain"), Some(75.0));
    }

    #[test]
    fn normalized_against_missing_required_without_default_fails() {
        let schema = test_schema(vec![
            float_parameter("gain", "Gain", None, None, 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ]);
        let ps = ParameterSet::default();
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn normalized_against_invalid_value_fails() {
        let schema = test_schema(vec![
            float_parameter("gain", "Gain", None, None, 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ]);
        let mut ps = ParameterSet::default();
        ps.insert("gain", ParameterValue::Float(200.0));
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn normalized_against_keeps_unknown_parameters() {
        let schema = test_schema(vec![
            float_parameter("gain", "Gain", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ]);
        let mut ps = ParameterSet::default();
        ps.insert("gain", ParameterValue::Float(50.0));
        ps.insert("unknown_param", ParameterValue::Float(99.0));
        let result = ps.normalized_against(&schema).unwrap();
        assert_eq!(result.get_f32("unknown_param"), Some(99.0));
    }

    // ── Builder functions ───────────────────────────────────────────

    #[test]
    fn float_parameter_builder() {
        let spec = float_parameter("gain", "Gain", Some("amp"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent);
        assert_eq!(spec.path, "gain");
        assert_eq!(spec.label, "Gain");
        assert_eq!(spec.group, Some("amp".to_string()));
        assert_eq!(spec.widget, ParameterWidget::Knob);
        assert_eq!(spec.unit, ParameterUnit::Percent);
        assert_eq!(spec.default_value, Some(ParameterValue::Float(50.0)));
        assert!(!spec.optional);
        assert!(!spec.allow_empty);
        assert!(matches!(spec.domain, ParameterDomain::FloatRange { min: 0.0, max: 100.0, step: 1.0 }));
    }

    #[test]
    fn bool_parameter_builder() {
        let spec = bool_parameter("bright", "Bright", None, Some(true));
        assert_eq!(spec.path, "bright");
        assert_eq!(spec.widget, ParameterWidget::Toggle);
        assert_eq!(spec.unit, ParameterUnit::None);
        assert_eq!(spec.domain, ParameterDomain::Bool);
        assert_eq!(spec.default_value, Some(ParameterValue::Bool(true)));
    }

    #[test]
    fn bool_parameter_builder_no_default() {
        let spec = bool_parameter("mute", "Mute", None, None);
        assert_eq!(spec.default_value, None);
    }

    #[test]
    fn enum_parameter_builder() {
        let spec = enum_parameter("key", "Key", Some("scale"), Some("C"), &[("C", "C Major"), ("D", "D Major")]);
        assert_eq!(spec.path, "key");
        assert_eq!(spec.group, Some("scale".to_string()));
        assert_eq!(spec.widget, ParameterWidget::Select);
        assert_eq!(spec.default_value, Some(ParameterValue::String("C".to_string())));
        if let ParameterDomain::Enum { options } = &spec.domain {
            assert_eq!(options.len(), 2);
            assert_eq!(options[0].value, "C");
            assert_eq!(options[0].label, "C Major");
        } else {
            panic!("expected Enum domain");
        }
    }

    #[test]
    fn text_parameter_builder() {
        let spec = text_parameter("name", "Name", None, Some("default"), true);
        assert_eq!(spec.path, "name");
        assert_eq!(spec.widget, ParameterWidget::TextInput);
        assert_eq!(spec.domain, ParameterDomain::Text);
        assert_eq!(spec.default_value, Some(ParameterValue::String("default".to_string())));
        assert!(spec.optional);
    }

    #[test]
    fn file_path_parameter_builder() {
        let spec = file_path_parameter("ir", "IR File", None, None, &["wav", "flac"], true);
        assert_eq!(spec.path, "ir");
        assert_eq!(spec.widget, ParameterWidget::FilePicker);
        assert!(spec.optional);
        if let ParameterDomain::FilePath { extensions } = &spec.domain {
            assert_eq!(extensions, &["wav", "flac"]);
        } else {
            panic!("expected FilePath domain");
        }
    }

    #[test]
    fn multi_slider_parameter_builder() {
        let spec = multi_slider_parameter("eq_band", "EQ Band", Some("eq"), Some(0.0), -24.0, 24.0, 0.1, ParameterUnit::Decibels);
        assert_eq!(spec.path, "eq_band");
        assert_eq!(spec.widget, ParameterWidget::MultiSlider);
        assert_eq!(spec.unit, ParameterUnit::Decibels);
        assert!(matches!(spec.domain, ParameterDomain::FloatRange { min: -24.0, max: 24.0, .. }));
    }

    #[test]
    fn curve_editor_parameter_builder() {
        let spec = curve_editor_parameter(
            "freq", "Frequency", None, CurveEditorRole::X,
            Some(1000.0), 20.0, 20000.0, 1.0, ParameterUnit::Hertz,
        );
        assert_eq!(spec.path, "freq");
        assert_eq!(spec.widget, ParameterWidget::CurveEditor { role: CurveEditorRole::X });
        assert_eq!(spec.unit, ParameterUnit::Hertz);
    }

    // ── required_f32 / required_bool / required_string ──────────────

    #[test]
    fn required_f32_success() {
        let mut ps = ParameterSet::default();
        ps.insert("gain", ParameterValue::Float(42.0));
        assert_eq!(required_f32(&ps, "gain").unwrap(), 42.0);
    }

    #[test]
    fn required_f32_missing_fails() {
        let ps = ParameterSet::default();
        assert!(required_f32(&ps, "gain").is_err());
    }

    #[test]
    fn required_f32_wrong_type_fails() {
        let mut ps = ParameterSet::default();
        ps.insert("gain", ParameterValue::String("hello".to_string()));
        assert!(required_f32(&ps, "gain").is_err());
    }

    #[test]
    fn required_bool_success() {
        let mut ps = ParameterSet::default();
        ps.insert("mute", ParameterValue::Bool(false));
        assert_eq!(required_bool(&ps, "mute").unwrap(), false);
    }

    #[test]
    fn required_bool_missing_fails() {
        let ps = ParameterSet::default();
        assert!(required_bool(&ps, "mute").is_err());
    }

    #[test]
    fn required_string_success() {
        let mut ps = ParameterSet::default();
        ps.insert("key", ParameterValue::String("C".to_string()));
        assert_eq!(required_string(&ps, "key").unwrap(), "C");
    }

    #[test]
    fn required_string_missing_fails() {
        let ps = ParameterSet::default();
        assert!(required_string(&ps, "key").is_err());
    }

    #[test]
    fn required_string_wrong_type_fails() {
        let mut ps = ParameterSet::default();
        ps.insert("key", ParameterValue::Int(5));
        assert!(required_string(&ps, "key").is_err());
    }

    // ── optional_string ─────────────────────────────────────────────

    #[test]
    fn optional_string_present() {
        let mut ps = ParameterSet::default();
        ps.insert("path", ParameterValue::String("foo.wav".to_string()));
        assert_eq!(optional_string(&ps, "path"), Some("foo.wav".to_string()));
    }

    #[test]
    fn optional_string_null() {
        let mut ps = ParameterSet::default();
        ps.insert("path", ParameterValue::Null);
        assert_eq!(optional_string(&ps, "path"), None);
    }

    #[test]
    fn optional_string_missing() {
        let ps = ParameterSet::default();
        assert_eq!(optional_string(&ps, "path"), None);
    }

    // ── ParameterDomain::value_kind ─────────────────────────────────

    #[test]
    fn value_kind_returns_correct_labels() {
        assert_eq!(ParameterDomain::Bool.value_kind(), "bool");
        assert_eq!(ParameterDomain::IntRange { min: 0, max: 10, step: 1 }.value_kind(), "int");
        assert_eq!(ParameterDomain::FloatRange { min: 0.0, max: 1.0, step: 0.1 }.value_kind(), "float");
        assert_eq!(ParameterDomain::Enum { options: vec![] }.value_kind(), "enum");
        assert_eq!(ParameterDomain::Text.value_kind(), "string");
        assert_eq!(ParameterDomain::FilePath { extensions: vec![] }.value_kind(), "path");
    }

    // ── BlockParameterDescriptor::validate_value ────────────────────

    #[test]
    fn block_parameter_descriptor_validate_delegates_to_spec() {
        let desc = BlockParameterDescriptor {
            id: ParameterId::for_block_path(&BlockId("test_block".to_string()), "gain"),
            block_id: BlockId("test_block".to_string()),
            effect_type: "preamp".to_string(),
            model: "test".to_string(),
            audio_mode: ModelAudioMode::MonoOnly,
            path: "gain".to_string(),
            label: "Gain".to_string(),
            group: None,
            widget: ParameterWidget::Knob,
            unit: ParameterUnit::Percent,
            domain: ParameterDomain::FloatRange { min: 0.0, max: 100.0, step: 1.0 },
            default_value: Some(ParameterValue::Float(50.0)),
            current_value: ParameterValue::Float(50.0),
            optional: false,
            allow_empty: false,
        };
        assert!(desc.validate_value(&ParameterValue::Float(75.0)).is_ok());
        assert!(desc.validate_value(&ParameterValue::Float(200.0)).is_err());
    }

    // ── ParameterSpec::materialize ──────────────────────────────────

    #[test]
    fn materialize_creates_correct_descriptor() {
        let spec = float_parameter("gain", "Gain", Some("amp"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent);
        let block_id = BlockId("test_block".to_string());
        let desc = spec.materialize(
            &block_id,
            "preamp",
            "test_model",
            ModelAudioMode::DualMono,
            ParameterValue::Float(75.0),
        );
        assert_eq!(desc.path, "gain");
        assert_eq!(desc.label, "Gain");
        assert_eq!(desc.group, Some("amp".to_string()));
        assert_eq!(desc.effect_type, "preamp");
        assert_eq!(desc.model, "test_model");
        assert_eq!(desc.audio_mode, ModelAudioMode::DualMono);
        assert_eq!(desc.current_value, ParameterValue::Float(75.0));
        assert_eq!(desc.default_value, Some(ParameterValue::Float(50.0)));
        assert_eq!(desc.block_id, block_id);
    }

    // ── Serde roundtrip for ParameterSet ────────────────────────────

    #[test]
    fn parameter_set_serde_roundtrip() {
        let mut ps = ParameterSet::default();
        ps.insert("gain", ParameterValue::Float(50.0));
        ps.insert("bright", ParameterValue::Bool(true));
        let json = serde_json::to_string(&ps).unwrap();
        let back: ParameterSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ps);
    }

    // ── ParameterWidget variants ────────────────────────────────────

    #[test]
    fn parameter_widget_equality() {
        assert_eq!(ParameterWidget::Knob, ParameterWidget::Knob);
        assert!(ParameterWidget::Knob != ParameterWidget::Toggle);
        assert_eq!(
            ParameterWidget::CurveEditor { role: CurveEditorRole::X },
            ParameterWidget::CurveEditor { role: CurveEditorRole::X }
        );
        assert!(
            ParameterWidget::CurveEditor { role: CurveEditorRole::X }
                != ParameterWidget::CurveEditor { role: CurveEditorRole::Y }
        );
    }

    // ── ParameterUnit variants ──────────────────────────────────────

    #[test]
    fn parameter_unit_serde_roundtrip() {
        let units = vec![
            ParameterUnit::None,
            ParameterUnit::Decibels,
            ParameterUnit::Hertz,
            ParameterUnit::Milliseconds,
            ParameterUnit::Percent,
            ParameterUnit::Ratio,
            ParameterUnit::Semitones,
        ];
        for unit in units {
            let json = serde_json::to_string(&unit).unwrap();
            let back: ParameterUnit = serde_json::from_str(&json).unwrap();
            assert_eq!(back, unit);
        }
    }

    // ── ModelParameterSchema serde ──────────────────────────────────

    #[test]
    fn model_parameter_schema_debug() {
        let schema = test_schema(vec![]);
        let dbg = format!("{:?}", schema);
        assert!(dbg.contains("test_model"));
    }
}
