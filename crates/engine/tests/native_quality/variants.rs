//! Responsibility: derives the parameter extremes a model is driven at.

use block_core::param::{ParameterDomain, ParameterSet};
use domain::value_objects::ParameterValue;

use super::catalog::NativeModel;

/// A parameter set with a label naming which knob is where.
pub struct Variant {
    pub label: String,
    pub params: ParameterSet,
}

/// Each knob alone at its minimum and maximum (bools both ways, every enum
/// option), the rest at default.
pub fn one_at_a_time(model: &NativeModel, defaults: &ParameterSet) -> Vec<Variant> {
    let mut out = Vec::new();
    for spec in &model.schema.parameters {
        let values: Vec<(String, ParameterValue)> = match &spec.domain {
            ParameterDomain::FloatRange { min, max, .. } => vec![
                (format!("{}=min", spec.path), ParameterValue::Float(*min)),
                (format!("{}=max", spec.path), ParameterValue::Float(*max)),
            ],
            ParameterDomain::IntRange { min, max, .. } => vec![
                (format!("{}=min", spec.path), ParameterValue::Int(*min)),
                (format!("{}=max", spec.path), ParameterValue::Int(*max)),
            ],
            ParameterDomain::Bool => vec![
                (format!("{}=off", spec.path), ParameterValue::Bool(false)),
                (format!("{}=on", spec.path), ParameterValue::Bool(true)),
            ],
            ParameterDomain::Enum { options } => options
                .iter()
                .map(|o| {
                    (
                        format!("{}={}", spec.path, o.value),
                        ParameterValue::String(o.value.clone()),
                    )
                })
                .collect(),
            ParameterDomain::Text | ParameterDomain::FilePath { .. } => Vec::new(),
        };
        for (label, value) in values {
            let mut params = defaults.clone();
            params.insert(spec.path.clone(), value);
            out.push(Variant { label, params });
        }
    }
    out
}

/// Every numeric knob at its maximum and every switch on: the worst case
/// for feedback, resonance and gain staging.
pub fn all_max(model: &NativeModel, defaults: &ParameterSet) -> ParameterSet {
    let mut params = defaults.clone();
    for spec in &model.schema.parameters {
        let value = match &spec.domain {
            ParameterDomain::FloatRange { max, .. } => ParameterValue::Float(*max),
            ParameterDomain::IntRange { max, .. } => ParameterValue::Int(*max),
            ParameterDomain::Bool => ParameterValue::Bool(true),
            _ => continue,
        };
        params.insert(spec.path.clone(), value);
    }
    params
}
