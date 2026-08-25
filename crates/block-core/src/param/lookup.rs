//! Responsibility: reads one typed parameter out of a set.

use super::set::ParameterSet;

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
