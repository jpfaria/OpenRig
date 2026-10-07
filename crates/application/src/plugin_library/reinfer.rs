//! Responsibility: rebuilds a capture plugin's parameters from its capture names.

use plugin_loader::manifest::{Backend, GridCapture, PluginManifest};

use super::manifest_grid::replace_grid;
use crate::tone3000::axes::{infer_axes, CaptureKind};

/// `manifest` with the parameters the capture names `names` (one per
/// capture, in order) infer, the way an install names them. Files, levels
/// and noise gates stay. `None` when it is not a capture plugin or the
/// names do not match its captures.
pub fn reinfer(manifest: &PluginManifest, names: &[String]) -> Option<PluginManifest> {
    let (kind, captures) = match &manifest.backend {
        Backend::Nam { captures, .. } => (CaptureKind::Nam, captures),
        Backend::Ir { captures, .. } => (CaptureKind::Ir, captures),
        _ => return None,
    };
    if names.len() != captures.len() {
        return None;
    }
    let axes = infer_axes(names, kind);
    let captures = captures
        .iter()
        .zip(axes.values)
        .map(|(capture, values)| GridCapture {
            values,
            ..capture.clone()
        })
        .collect();
    replace_grid(manifest, axes.parameters, captures)
}
