//! Schema-level validation for [`PluginManifest`].
//!
//! Catches problems that `serde` parsing alone won't reject: empty fields,
//! unsupported manifest versions, NAM/IR grids that don't match their
//! parameters, LV2 manifests with no binary slots, and so on.
//!
//! Filesystem-level checks (the referenced `.nam`/`.wav`/`.lv2` actually
//! exist and load) live in a follow-up module — keeping pure validation
//! separate makes it cheap to call from any context (CI, dry-run, tests).
//!
//! Issue: #287

use std::collections::{BTreeMap, HashSet};

use crate::manifest::{Backend, GridCapture, GridParameter, ParameterValue, PluginManifest};

/// Highest `manifest_version` this loader understands.
pub const MAX_SUPPORTED_VERSION: u32 = 1;

/// Reasons a manifest may be rejected.
///
/// Each variant carries enough context to point a human at the problem
/// without needing access to the full manifest.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ValidationError {
    #[error("manifest_version {found} is newer than supported {max}")]
    UnsupportedVersion { found: u32, max: u32 },

    #[error("`id` must not be empty")]
    EmptyId,

    #[error("`display_name` must not be empty")]
    EmptyDisplayName,

    #[error("LV2 plugin `plugin_uri` must not be empty")]
    EmptyLv2PluginUri,

    #[error("LV2 plugin must declare at least one platform binary slot")]
    NoLv2Slots,

    #[error(
        "capture #{capture_index} references unknown parameter `{parameter}` \
         (declared parameters: {declared:?})"
    )]
    UnknownCaptureParameter {
        capture_index: usize,
        parameter: String,
        declared: Vec<String>,
    },

    #[error(
        "capture #{capture_index} sets parameter `{parameter}` = {value:?}, \
         which is not among its declared values {declared:?}"
    )]
    InvalidCaptureValue {
        capture_index: usize,
        parameter: String,
        value: ParameterValue,
        declared: Vec<ParameterValue>,
    },

    #[error(
        "capture #{capture_index} is missing parameter `{parameter}` \
         (every capture must set every declared parameter)"
    )]
    MissingCaptureParameter {
        capture_index: usize,
        parameter: String,
    },

    #[error("capture grid has duplicate entries (cells covered more than once)")]
    DuplicateCaptures,

    #[error("NAM/IR backend declares parameters but ships zero captures")]
    EmptyCaptureGrid,

    #[error("parameter `{name}` declares no values")]
    EmptyParameterValues { name: String },

    #[error("parameter name must not be empty")]
    EmptyParameterName,

    #[error("VST3 plugin `bundle` path must not be empty")]
    EmptyVst3Bundle,

    #[error("VST3 parameter `{name}` declares max < min")]
    Vst3InvalidRange { name: String },

    #[error("Native plugin `runtime_id` must not be empty")]
    EmptyNativeRuntimeId,
}

/// Validate a manifest's internal consistency.
///
/// Returns `Ok(())` when the manifest is well-formed. Filesystem checks
/// (referenced files exist, LV2 binaries load) are not performed here.
pub fn validate_manifest(manifest: &PluginManifest) -> Result<(), ValidationError> {
    if manifest.manifest_version > MAX_SUPPORTED_VERSION {
        return Err(ValidationError::UnsupportedVersion {
            found: manifest.manifest_version,
            max: MAX_SUPPORTED_VERSION,
        });
    }
    if manifest.id.trim().is_empty() {
        return Err(ValidationError::EmptyId);
    }
    if manifest.display_name.trim().is_empty() {
        return Err(ValidationError::EmptyDisplayName);
    }

    match &manifest.backend {
        Backend::Native { runtime_id } => {
            if runtime_id.trim().is_empty() {
                return Err(ValidationError::EmptyNativeRuntimeId);
            }
            Ok(())
        }
        Backend::Nam {
            parameters,
            captures,
        }
        | Backend::Ir {
            parameters,
            captures,
        } => validate_grid(parameters, captures),
        Backend::Lv2 {
            plugin_uri,
            binaries,
        } => {
            if plugin_uri.trim().is_empty() {
                return Err(ValidationError::EmptyLv2PluginUri);
            }
            if binaries.is_empty() {
                return Err(ValidationError::NoLv2Slots);
            }
            Ok(())
        }
        Backend::Vst3 { bundle, parameters } => {
            if bundle.as_os_str().is_empty() {
                return Err(ValidationError::EmptyVst3Bundle);
            }
            for parameter in parameters {
                if parameter.name.trim().is_empty() {
                    return Err(ValidationError::EmptyParameterName);
                }
                if parameter.max < parameter.min {
                    return Err(ValidationError::Vst3InvalidRange {
                        name: parameter.name.clone(),
                    });
                }
            }
            Ok(())
        }
    }
}

fn validate_grid(
    parameters: &[GridParameter],
    captures: &[GridCapture],
) -> Result<(), ValidationError> {
    for parameter in parameters {
        if parameter.name.trim().is_empty() {
            return Err(ValidationError::EmptyParameterName);
        }
        if parameter.values.is_empty() {
            return Err(ValidationError::EmptyParameterValues {
                name: parameter.name.clone(),
            });
        }
    }

    let parameter_names: BTreeMap<&str, &[ParameterValue]> = parameters
        .iter()
        .map(|parameter| (parameter.name.as_str(), parameter.values.as_slice()))
        .collect();

    let mut seen_keys: HashSet<Vec<(String, ParameterValue)>> = HashSet::new();

    for (index, capture) in captures.iter().enumerate() {
        for (name, value) in &capture.values {
            let Some(declared_values) = parameter_names.get(name.as_str()) else {
                return Err(ValidationError::UnknownCaptureParameter {
                    capture_index: index,
                    parameter: name.clone(),
                    declared: parameters
                        .iter()
                        .map(|parameter| parameter.name.clone())
                        .collect(),
                });
            };
            if !declared_values.iter().any(|declared| declared == value) {
                return Err(ValidationError::InvalidCaptureValue {
                    capture_index: index,
                    parameter: name.clone(),
                    value: value.clone(),
                    declared: declared_values.to_vec(),
                });
            }
        }

        for parameter in parameters {
            if !capture.values.contains_key(&parameter.name) {
                return Err(ValidationError::MissingCaptureParameter {
                    capture_index: index,
                    parameter: parameter.name.clone(),
                });
            }
        }

        seen_keys.insert(canonical_key(&capture.values));
    }

    if seen_keys.len() != captures.len() {
        return Err(ValidationError::DuplicateCaptures);
    }
    if !parameters.is_empty() && captures.is_empty() {
        return Err(ValidationError::EmptyCaptureGrid);
    }

    Ok(())
}

fn canonical_key(values: &BTreeMap<String, ParameterValue>) -> Vec<(String, ParameterValue)> {
    values
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;
