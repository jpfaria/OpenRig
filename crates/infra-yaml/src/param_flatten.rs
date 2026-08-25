//! Responsibility: flattens a nested parameter document into a flat parameter set.
//!
//! Split out of `lib.rs` (#873). YAML nests parameters as mappings; the
//! runtime addresses them by dotted path, so the two shapes have to be
//! translated in both directions.

use anyhow::{anyhow, Result};
use serde_yaml::Value;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::param::ParameterSet;

pub(crate) fn flatten_parameter_set(value: Value) -> Result<ParameterSet> {
    let mut params = ParameterSet::default();
    match value {
        Value::Null => Ok(params),
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                let key = yaml_key_to_string(key)?;
                flatten_parameter_value(&mut params, &key, value)?;
            }
            Ok(params)
        }
        other => Err(anyhow!("params must be a mapping, got {:?}", other)),
    }
}

fn flatten_parameter_value(params: &mut ParameterSet, path: &str, value: Value) -> Result<()> {
    match value {
        Value::Mapping(mapping) => {
            for (key, nested_value) in mapping {
                let key = yaml_key_to_string(key)?;
                let nested_path = format!("{}.{}", path, key);
                flatten_parameter_value(params, &nested_path, nested_value)?;
            }
            Ok(())
        }
        scalar => {
            params.insert(path.to_string(), yaml_scalar_to_parameter_value(scalar)?);
            Ok(())
        }
    }
}

pub(crate) fn parameter_set_to_yaml_value(params: &ParameterSet) -> Value {
    let mut root = serde_yaml::Mapping::new();
    for (path, value) in &params.values {
        let parts = path.split('.').collect::<Vec<_>>();
        insert_yaml_value(&mut root, &parts, parameter_value_to_yaml(value));
    }
    Value::Mapping(root)
}

pub(crate) fn insert_yaml_value(mapping: &mut serde_yaml::Mapping, path: &[&str], value: Value) {
    if path.is_empty() {
        return;
    }
    let key = Value::String(path[0].to_string());
    if path.len() == 1 {
        mapping.insert(key, value);
        return;
    }

    if !matches!(mapping.get(&key), Some(Value::Mapping(_))) {
        mapping.insert(key.clone(), Value::Mapping(serde_yaml::Mapping::new()));
    }

    if let Some(Value::Mapping(child)) = mapping.get_mut(&key) {
        insert_yaml_value(child, &path[1..], value);
    }
}

fn parameter_value_to_yaml(value: &ParameterValue) -> Value {
    match value {
        ParameterValue::Null => Value::Null,
        ParameterValue::Bool(value) => Value::Bool(*value),
        ParameterValue::Int(value) => serde_yaml::to_value(value).unwrap_or(Value::Null),
        ParameterValue::Float(value) => serde_yaml::to_value(value).unwrap_or(Value::Null),
        ParameterValue::String(value) => Value::String(value.clone()),
    }
}

pub(crate) fn yaml_key_to_string(value: Value) -> Result<String> {
    match value {
        Value::String(value) => Ok(value),
        other => Err(anyhow!("yaml object keys must be strings, got {:?}", other)),
    }
}

pub(crate) fn yaml_scalar_to_parameter_value(value: Value) -> Result<ParameterValue> {
    match value {
        Value::Null => Ok(ParameterValue::Null),
        Value::Bool(value) => Ok(ParameterValue::Bool(value)),
        Value::Number(value) => {
            if let Some(number) = value.as_i64() {
                Ok(ParameterValue::Int(number))
            } else if let Some(number) = value.as_f64() {
                Ok(ParameterValue::Float(number as f32))
            } else {
                Err(anyhow!("unsupported yaml number '{}'", value))
            }
        }
        Value::String(value) => Ok(ParameterValue::String(value)),
        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_) => {
            Err(anyhow!("unsupported yaml value in params"))
        }
    }
}

pub(crate) fn generated_block_id(chain_id: &ChainId, index: usize) -> BlockId {
    BlockId(format!("{}:block:{}", chain_id.0, index))
}
