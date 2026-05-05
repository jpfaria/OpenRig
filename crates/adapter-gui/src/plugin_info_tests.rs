
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
