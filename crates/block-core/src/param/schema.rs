//! Parameter schema types — describe a model's parameter contract:
//! widget, unit, value domain, default, optional/allow-empty flags, plus
//! the per-spec value validation that backs `ParameterSet::normalized_against`.
//!
//! Lifted out of `param.rs` (Phase 6 of issue #194). Schema lives between
//! the pure domain (`ParameterSet`) and the GUI-bound descriptor
//! (`BlockParameterDescriptor`) — it carries the contract a runtime
//! parameter must satisfy plus the metadata the UI needs to render it.

use domain::ids::{BlockId, ParameterId};
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};

use super::descriptor::BlockParameterDescriptor;
use crate::audio_types::ModelAudioMode;

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
    pub fn value_kind(&self) -> &'static str {
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
