//! `ParameterSet` — the runtime container of a model's parameter values
//! (path → `ParameterValue`), plus the typed lookup helpers and the
//! `normalized_against` schema-validation entry point.
//!
//! Lifted out of `param.rs` (Phase 6 of issue #194). Domain layer — no
//! UI metadata.

use std::collections::BTreeMap;

use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};

use super::schema::ModelParameterSchema;

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
                    path,
                    schema.effect_type,
                    schema.model
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
