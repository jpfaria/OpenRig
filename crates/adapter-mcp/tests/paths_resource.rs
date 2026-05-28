//! Red-first (#582) tests for the `openrig://paths` MCP resource.
//!
//! Exposes the effective resolved system paths (data root + every
//! configurable directory) so skills and other MCP clients read the
//! target locations dynamically instead of hard-coding OS-specific
//! defaults.

use std::path::PathBuf;

use adapter_mcp::resources::URI_PATHS;
use application::query::ResolvedPaths;
use infra_filesystem::AssetPaths;
use serde_json::Value;

#[test]
fn paths_uri_constant_matches_spec() {
    assert_eq!(URI_PATHS, "openrig://paths");
}

#[test]
fn paths_envelope_defaults_to_os_paths_under_data_root() {
    // No override anywhere → every path resolves to a subfolder of the
    // OS data root. The wire shape must always carry an absolute path
    // (never `null`) so MCP clients don't re-implement the default
    // fallback themselves.
    let paths = AssetPaths::default();
    let resolved = ResolvedPaths::from_app_config(&paths);
    let json: Value = serde_json::from_str(&resolved.to_json()).expect("valid JSON envelope");
    assert!(
        json.get("data_root").and_then(Value::as_str).is_some(),
        "envelope must carry data_root"
    );
    for key in ["presets_path", "plugins_path", "evaluations_path"] {
        let v = json
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("envelope missing {key}: {json}"));
        assert!(
            v.starts_with(&resolved.data_root),
            "default {key} must live under data_root ({}): got {v}",
            resolved.data_root
        );
    }
}

#[test]
fn paths_envelope_echoes_overrides_for_every_path() {
    // Set every override → the envelope reports the user's pick
    // verbatim, not the OS default. Same contract as
    // `set_evaluations_path_persists_to_config_yaml` from the GUI
    // side, but from the resource shape's point of view.
    let presets = PathBuf::from("/custom/presets-582");
    let plugins = PathBuf::from("/custom/plugins-582");
    let evaluations = PathBuf::from("/custom/evaluations-582");
    let paths = AssetPaths {
        presets_path: Some(presets.clone()),
        plugins_path: Some(plugins.clone()),
        evaluations_path: Some(evaluations.clone()),
        ..AssetPaths::default()
    };
    let resolved = ResolvedPaths::from_app_config(&paths);
    assert_eq!(resolved.presets_path, presets.to_string_lossy());
    assert_eq!(resolved.plugins_path, plugins.to_string_lossy());
    assert_eq!(resolved.evaluations_path, evaluations.to_string_lossy());
}
