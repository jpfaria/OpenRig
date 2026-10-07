//! Responsibility: brings a loaded block's capture values up to its plugin's current grid.
//!
//! A project or preset saved before a plugin's parameters were edited
//! names a capture with the values of an older version. On load those
//! values are looked up in the saved versions, newest first, and the block
//! moves to the same capture file in the current grid.

use plugin_loader::manifest::{Backend, GridCapture, GridParameter, PluginManifest};
use plugin_loader::version_store::{read_version, version_numbers};

use super::grid_follow::{exact_capture, follow_capture};
use crate::param::ParameterSet;

/// `params` of a block of `model`, moved to the current grid when they name
/// a capture of an older saved version; otherwise unchanged.
pub fn follow_saved_grid_versions(model: &str, params: ParameterSet) -> ParameterSet {
    let Some(package) = plugin_loader::registry::find(model) else {
        return params;
    };
    let Some((parameters, captures)) = capture_grid(&package.manifest) else {
        return params;
    };
    if exact_capture(parameters, captures, &params).is_some() {
        return params;
    }
    for version in version_numbers(&package.root).into_iter().rev() {
        let Some(old) = read_version(&package.root, version) else {
            continue;
        };
        let Some((old_parameters, old_captures)) = capture_grid(&old) else {
            continue;
        };
        if let Some(moved) =
            follow_capture(old_parameters, old_captures, parameters, captures, &params)
        {
            return moved;
        }
    }
    params
}

/// The capture grid of a NAM or IR manifest.
pub fn capture_grid(manifest: &PluginManifest) -> Option<(&[GridParameter], &[GridCapture])> {
    match &manifest.backend {
        Backend::Nam {
            parameters,
            captures,
        }
        | Backend::Ir {
            parameters,
            captures,
        } => Some((parameters, captures)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "grid_version_follow_tests.rs"]
mod tests;
