//! `BlockParameterDescriptor` — a concrete instance of a parameter for a
//! specific block in a chain. Carries the spec's metadata plus the
//! current value, materialised by `ParameterSpec::materialize`.
//!
//! Lifted out of `param.rs` (Phase 6 of issue #194). UI-bound: this is
//! what the GUI consumes to render and edit a parameter.

use domain::ids::{BlockId, ParameterId};
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};

use super::schema::{ParameterDomain, ParameterSpec, ParameterUnit, ParameterWidget};
use crate::audio_types::ModelAudioMode;

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
