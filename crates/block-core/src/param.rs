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

#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
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
#[path = "param_tests.rs"]
mod tests;
