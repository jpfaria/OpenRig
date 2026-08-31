use super::*;

// --- PluginMetadata default ---

#[test]
fn plugin_metadata_default_has_empty_fields() {
    let meta = PluginMetadata::default();
    assert!(meta.description.is_empty());
    assert!(meta.license.is_empty());
    assert!(meta.homepage.is_empty());
}

// --- PluginMetadata clone ---

#[test]
fn plugin_metadata_clone_preserves_all_fields() {
    let meta = PluginMetadata {
        description: "A test plugin".to_string(),
        license: "MIT".to_string(),
        homepage: "https://example.com".to_string(),
    };
    let cloned = meta.clone();
    assert_eq!(cloned.description, "A test plugin");
    assert_eq!(cloned.license, "MIT");
    assert_eq!(cloned.homepage, "https://example.com");
}

// --- PluginMetadata deserialization ---

#[test]
fn plugin_metadata_deserialize_empty_yaml_uses_defaults() {
    let yaml = "{}";
    let meta: PluginMetadata = serde_yaml::from_str(yaml).unwrap();
    assert!(meta.description.is_empty());
    assert!(meta.license.is_empty());
    assert!(meta.homepage.is_empty());
}

#[test]
fn plugin_metadata_deserialize_partial_yaml() {
    let yaml = "description: Some desc";
    let meta: PluginMetadata = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(meta.description, "Some desc");
    assert!(meta.license.is_empty());
}

#[test]
fn plugin_metadata_deserialize_full_yaml() {
    let yaml = r#"
description: A great plugin
license: GPL-3.0
homepage: https://example.com/plugin
"#;
    let meta: PluginMetadata = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(meta.description, "A great plugin");
    assert_eq!(meta.license, "GPL-3.0");
    assert_eq!(meta.homepage, "https://example.com/plugin");
}

// --- MetadataFile deserialization ---

#[test]
fn metadata_file_deserialize_with_plugins() {
    let yaml = r#"
plugins:
  my_plugin:
    description: Test desc
    license: MIT
    homepage: https://test.com
  another:
    description: Another one
"#;
    let file: super::MetadataFile = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(file.plugins.len(), 2);
    assert_eq!(file.plugins["my_plugin"].description, "Test desc");
    assert_eq!(file.plugins["my_plugin"].license, "MIT");
    assert_eq!(file.plugins["another"].description, "Another one");
    assert!(file.plugins["another"].license.is_empty());
}

#[test]
fn metadata_file_deserialize_empty_plugins() {
    let yaml = "plugins: {}";
    let file: super::MetadataFile = serde_yaml::from_str(yaml).unwrap();
    assert!(file.plugins.is_empty());
}

// ── #913: the lookup paths themselves, not just the DTOs ──────────────────

/// The lookups read `asset_paths()`, which panics until startup has set it.
/// Defaults point at directories that do not exist in a test run, so every
/// lookup below takes the "not found" path — which is what we assert.
fn asset_paths_ready() {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
}

#[test]
fn an_unknown_model_has_no_metadata_rather_than_a_placeholder() {
    asset_paths_ready();
    let meta = plugin_metadata("en-US", "no_such_model_913");
    assert!(meta.description.is_empty());
    assert!(meta.license.is_empty());
    assert!(meta.homepage.is_empty());
}

#[test]
fn an_unknown_language_falls_back_to_empty_metadata_not_a_panic() {
    asset_paths_ready();
    let meta = plugin_metadata("xx-YY", "no_such_model_913");
    assert!(meta.description.is_empty());
}

#[test]
fn the_metadata_cache_answers_the_same_way_on_the_second_call() {
    asset_paths_ready();
    // The YAML is read at most once per language; a second lookup must not
    // change the answer (a cache that stored the miss as a hit would).
    let first = plugin_metadata("en-US", "no_such_model_913");
    let second = plugin_metadata("en-US", "no_such_model_913");
    assert_eq!(first.description, second.description);
    assert_eq!(first.license, second.license);
}

#[test]
fn an_unknown_model_has_no_screenshot_so_the_caller_draws_the_placeholder() {
    asset_paths_ready();
    assert!(super::screenshot_png("gain", "no_such_model_913").is_none());
    assert!(super::screenshot_png("", "no_such_model_913").is_none());
}

#[test]
fn an_empty_homepage_opens_nothing() {
    // The guard is the whole point: without it an empty field would hand
    // the OS an empty URL and pop a browser window on the user's desktop.
    super::open_homepage("");
}
