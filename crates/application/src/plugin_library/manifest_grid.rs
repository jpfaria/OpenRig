//! Responsibility: swaps the capture grid of a capture plugin's manifest.

use plugin_loader::manifest::{Backend, GridCapture, GridParameter, PluginManifest};

/// `manifest` with `parameters` and `captures` in place of its own, or
/// `None` when it is not a capture plugin (NAM or IR).
pub fn replace_grid(
    manifest: &PluginManifest,
    parameters: Vec<GridParameter>,
    captures: Vec<GridCapture>,
) -> Option<PluginManifest> {
    let backend = match &manifest.backend {
        Backend::Nam { .. } => Backend::Nam {
            parameters,
            captures,
        },
        Backend::Ir { .. } => Backend::Ir {
            parameters,
            captures,
        },
        _ => return None,
    };
    Some(PluginManifest {
        backend,
        ..manifest.clone()
    })
}
