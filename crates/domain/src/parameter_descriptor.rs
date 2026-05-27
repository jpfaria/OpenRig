//! Domain-owned schema of a block parameter: id, kind, default.
//!
//! Issue #572 / spec
//! `docs/superpowers/specs/2026-05-27-issue-572-mcp-block-plugin-params-design.md`.
//! Single source of truth for every transport that needs to introspect the
//! parameters a block exposes — GUI consumes it the same way MCP and gRPC will.

use crate::ids::ParameterId;
use crate::value_objects::ParameterValue;

#[derive(Debug, Clone, PartialEq)]
pub enum ParameterKind {
    Number { min: f32, max: f32, step: f32 },
    Bool,
    Text,
    Option { values: Vec<String> },
    File,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParameterDescriptor {
    pub id: ParameterId,
    pub kind: ParameterKind,
    pub default: ParameterValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DescriptorError {
    NumberMinNotLessThanMax { min: f32, max: f32 },
    NumberStepNotPositive { step: f32 },
    NumberDefaultOutOfRange { default: f32, min: f32, max: f32 },
    NumberDefaultNotNumeric,
    BoolDefaultNotBool,
    TextDefaultNotString,
    OptionEmptyValues,
    OptionDefaultNotInValues { default: String },
    OptionDefaultNotString,
    FileDefaultNotPathOrNull,
}

impl ParameterDescriptor {
    pub fn number(
        id: ParameterId,
        min: f32,
        max: f32,
        step: f32,
        default: ParameterValue,
    ) -> Result<Self, DescriptorError> {
        if !(min < max) {
            return Err(DescriptorError::NumberMinNotLessThanMax { min, max });
        }
        if !(step > 0.0) {
            return Err(DescriptorError::NumberStepNotPositive { step });
        }
        let default_value = default
            .as_f32()
            .ok_or(DescriptorError::NumberDefaultNotNumeric)?;
        if default_value < min || default_value > max {
            return Err(DescriptorError::NumberDefaultOutOfRange {
                default: default_value,
                min,
                max,
            });
        }
        Ok(Self {
            id,
            kind: ParameterKind::Number { min, max, step },
            default,
        })
    }

    pub fn bool(id: ParameterId, default: ParameterValue) -> Result<Self, DescriptorError> {
        if default.as_bool().is_none() {
            return Err(DescriptorError::BoolDefaultNotBool);
        }
        Ok(Self {
            id,
            kind: ParameterKind::Bool,
            default,
        })
    }

    pub fn text(id: ParameterId, default: ParameterValue) -> Result<Self, DescriptorError> {
        if default.as_str().is_none() {
            return Err(DescriptorError::TextDefaultNotString);
        }
        Ok(Self {
            id,
            kind: ParameterKind::Text,
            default,
        })
    }

    pub fn option(
        id: ParameterId,
        values: Vec<String>,
        default: ParameterValue,
    ) -> Result<Self, DescriptorError> {
        if values.is_empty() {
            return Err(DescriptorError::OptionEmptyValues);
        }
        let default_str = default
            .as_str()
            .ok_or(DescriptorError::OptionDefaultNotString)?;
        if !values.iter().any(|v| v == default_str) {
            return Err(DescriptorError::OptionDefaultNotInValues {
                default: default_str.to_string(),
            });
        }
        Ok(Self {
            id,
            kind: ParameterKind::Option { values },
            default,
        })
    }

    pub fn file(id: ParameterId, default: ParameterValue) -> Result<Self, DescriptorError> {
        match &default {
            ParameterValue::String(_) | ParameterValue::Null => Ok(Self {
                id,
                kind: ParameterKind::File,
                default,
            }),
            _ => Err(DescriptorError::FileDefaultNotPathOrNull),
        }
    }
}
