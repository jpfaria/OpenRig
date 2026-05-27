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
        Ok(Self {
            id,
            kind: ParameterKind::Number { min, max, step },
            default,
        })
    }
}
