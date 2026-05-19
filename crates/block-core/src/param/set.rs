//! `ParameterSet` — the runtime container of a model's parameter values
//! (path → `ParameterValue`), plus the typed lookup helpers and the
//! `normalized_against` schema-validation entry point.
//!
//! Lifted out of `param.rs` (Phase 6 of issue #194). Domain layer — no
//! UI metadata.

use std::collections::BTreeMap;

use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};

use super::schema::{ModelParameterSchema, ParameterDomain, ParameterSpec};

/// Coerce a stored `ParameterValue` to the type the schema expects, when
/// the conversion is unambiguous. Backward-compat for projects saved
/// before issue #401 introduced Enum/Bool routing — those projects have
/// every LV2 control stored as Float regardless of the underlying TTL
/// port property.
///
/// Round-trips:
/// - Float/Int stored against an `Enum` schema → match by numeric option
///   value, return the corresponding `String(option.value)`.
/// - Float/Int stored against `Bool` → `>= 0.5` → `Bool`.
/// - Anything else → return the value unchanged (the validator will run
///   normally and surface a real type mismatch when it's a real bug).
fn coerce_legacy_value(value: &ParameterValue, spec: &ParameterSpec) -> ParameterValue {
    let numeric = match value {
        ParameterValue::Float(v) => Some(*v as f64),
        ParameterValue::Int(v) => Some(*v as f64),
        _ => None,
    };
    let Some(numeric) = numeric else {
        return value.clone();
    };
    match &spec.domain {
        ParameterDomain::Enum { options } => {
            // Option values are stored stringified — match by numeric
            // proximity (handles `"0"` vs `"0.0"` round-trips from
            // f32::to_string).
            for option in options {
                if let Ok(parsed) = option.value.parse::<f64>() {
                    if (parsed - numeric).abs() < 1e-6 {
                        return ParameterValue::String(option.value.clone());
                    }
                }
            }
            value.clone()
        }
        ParameterDomain::Bool => ParameterValue::Bool(numeric >= 0.5),
        _ => value.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, schemars::JsonSchema)]
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
        self.normalized_against_internal(schema, false)
    }

    /// Strict version of [`normalized_against`] — rejects unknown parameters
    /// instead of warning. Issue #400 bug #5: surfaces schema mismatch
    /// loudly so silent contract drift (e.g. preset using `high_cut`/`low_cut`
    /// against `native_guitar_eq` whose schema is `low`/`low_mid`/`high_mid`/`high`)
    /// is caught at load time, not silently muted into a no-op processor.
    ///
    /// Use this in tests, in CI validation, and at preset-load time for new
    /// presets. Existing user-saved presets continue to use the lenient
    /// `normalized_against` so legacy snapshots survive model version bumps.
    pub fn normalized_strict(&self, schema: &ModelParameterSchema) -> Result<Self, String> {
        self.normalized_against_internal(schema, true)
    }

    fn normalized_against_internal(
        &self,
        schema: &ModelParameterSchema,
        strict: bool,
    ) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        let mut known_specs = BTreeMap::new();
        for spec in &schema.parameters {
            known_specs.insert(spec.path.as_str(), spec);
        }

        for (path, value) in &self.values {
            let Some(spec) = known_specs.get(path.as_str()) else {
                if strict {
                    return Err(format!(
                        "unknown parameter '{}' for {} model '{}' (schema does not declare it; \
                         may indicate a stale preset or a renamed param)",
                        path, schema.effect_type, schema.model
                    ));
                }
                // Lenient: keep unknown parameters instead of silently dropping
                // them. They may belong to a different version of the model or
                // be internal state that should survive round-trips.
                log::warn!(
                    "[param] keeping unknown parameter '{}' (not in schema for {} model '{}')",
                    path,
                    schema.effect_type,
                    schema.model
                );
                values.insert(path.clone(), value.clone());
                continue;
            };
            // Backward-compat: projects saved before issue #401 stored
            // every LV2 control as Float, including ones we now route
            // to Enum/Bool (lv2:enumeration / lv2:toggled). Coerce
            // numeric stored values into the spec's expected type when
            // possible — without this every chain block with a toggle
            // or enum gets silently dropped on load (#401).
            let coerced = coerce_legacy_value(value, spec);
            spec.validate_value(&coerced).map_err(|error| {
                format!(
                    "invalid parameter '{}' for {} model '{}': {}",
                    path, schema.effect_type, schema.model, error
                )
            })?;
            values.insert(path.clone(), coerced);
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
