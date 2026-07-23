//! Tests for `infra-filesystem`. Lifted from `lib.rs` so the production file
//! stays under the size cap. Re-attached via `#[cfg(test)] #[path] mod tests;`.

use super::*;
use std::fs;
use std::path::PathBuf;

// ── helpers ──────────────────────────────────────────────────────────

/// Create a unique temporary directory for each test.
pub(super) fn tmp_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("openrig_tests")
        .join(test_name)
        .join(format!("{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create tmp dir");
    dir
}

fn insert_recent_project(config: &mut AppConfig, entry: RecentProjectEntry) {
    config
        .recent_projects
        .retain(|current| current.project_path != entry.project_path);
    config.recent_projects.insert(0, entry);
}

fn make_entry(path: &str, name: &str) -> RecentProjectEntry {
    RecentProjectEntry {
        project_path: path.into(),
        project_name: name.into(),
        is_valid: true,
        invalid_reason: None,
    }
}

pub(super) fn make_device(id: &str, name: &str) -> GuiAudioDeviceSettings {
    GuiAudioDeviceSettings {
        device_id: id.into(),
        name: name.into(),
        sample_rate: 48_000,
        buffer_size_frames: 256,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

// ── AssetPaths ──────────────────────────────────────────────────────

#[test]
fn asset_paths_default_fields_not_empty() {
    let paths = AssetPaths::default();
    assert!(!paths.thumbnails.is_empty());
    assert!(!paths.screenshots.is_empty());
    assert!(!paths.metadata.is_empty());
}

#[test]
fn asset_paths_serde_roundtrip_preserves_values() {
    let paths = AssetPaths {
        thumbnails: "custom/thumbs".into(),
        screenshots: "custom/screens".into(),
        metadata: "custom/meta".into(),
        presets_path: None,
        plugins_path: None,
        evaluations_path: None,
    };
    let yaml = serde_yaml::to_string(&paths).unwrap();
    let restored: AssetPaths = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(paths, restored);
}

#[test]
fn asset_paths_deserialize_empty_yaml_uses_defaults() {
    let paths: AssetPaths = serde_yaml::from_str("{}").unwrap();
    let default = AssetPaths::default();
    assert_eq!(paths, default);
}

#[test]
fn resolve_asset_paths_absolute_left_unchanged() {
    let paths = AssetPaths {
        thumbnails: "/absolute/thumbs".into(),
        screenshots: "/absolute/screens".into(),
        metadata: "/absolute/meta".into(),
        presets_path: None,
        plugins_path: None,
        evaluations_path: None,
    };
    let resolved = resolve_asset_paths(paths.clone());
    assert_eq!(resolved.thumbnails, "/absolute/thumbs");
    assert_eq!(resolved.screenshots, "/absolute/screens");
    assert_eq!(resolved.metadata, "/absolute/meta");
}

#[test]
fn resolve_asset_paths_relative_gets_root_prepended() {
    let paths = AssetPaths::default();
    let resolved = resolve_asset_paths(paths.clone());
    assert!(
        std::path::Path::new(&resolved.thumbnails).is_absolute()
            || resolved.thumbnails.contains('/'),
        "resolved thumbnails should have root prepended: {}",
        resolved.thumbnails
    );
    assert!(
        resolved.thumbnails.ends_with(&paths.thumbnails),
        "resolved path '{}' should end with '{}'",
        resolved.thumbnails,
        paths.thumbnails
    );
}

// ── RecentProjectEntry ──────────────────────────────────────────────

#[test]
fn recent_project_entry_default_is_valid_false() {
    // Rust `Default` for bool is false; the serde `default_true` only
    // applies when deserializing from YAML with a missing field.
    let entry = RecentProjectEntry::default();
    assert!(!entry.is_valid);
    assert!(entry.invalid_reason.is_none());
}

#[test]
fn recent_project_entry_serde_default_is_valid_true() {
    // When deserializing without `is_valid`, serde uses `default_true`
    let yaml = "project_path: /x\nproject_name: X\n";
    let entry: RecentProjectEntry = serde_yaml::from_str(yaml).unwrap();
    assert!(entry.is_valid);
}

#[test]
fn recent_project_entry_serde_roundtrip() {
    let entry = RecentProjectEntry {
        project_path: "/some/path.yaml".into(),
        project_name: "My Project".into(),
        is_valid: false,
        invalid_reason: Some("file not found".into()),
    };
    let yaml = serde_yaml::to_string(&entry).unwrap();
    let restored: RecentProjectEntry = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(entry, restored);
}

#[test]
fn recent_project_entry_deserialize_minimal_yaml_defaults() {
    let yaml = "project_path: /x\nproject_name: X\n";
    let entry: RecentProjectEntry = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(entry.project_path, "/x");
    assert_eq!(entry.project_name, "X");
    assert!(entry.is_valid); // default_true
    assert!(entry.invalid_reason.is_none());
}

#[test]
fn recent_projects_move_existing_entry_to_top_without_duplication() {
    let mut config = AppConfig {
        recent_projects: vec![
            make_entry("/b/project.yaml", "B"),
            make_entry("/a/project.yaml", "A"),
        ],
        ..Default::default()
    };

    insert_recent_project(&mut config, make_entry("/a/project.yaml", "A2"));

    assert_eq!(config.recent_projects.len(), 2);
    assert_eq!(config.recent_projects[0].project_path, "/a/project.yaml");
    assert_eq!(config.recent_projects[0].project_name, "A2");
    assert_eq!(config.recent_projects[1].project_path, "/b/project.yaml");
}

#[test]
fn recent_projects_insert_new_entry_at_top() {
    let mut config = AppConfig {
        recent_projects: vec![make_entry("/a/project.yaml", "A")],
        ..Default::default()
    };

    insert_recent_project(&mut config, make_entry("/b/project.yaml", "B"));

    assert_eq!(config.recent_projects.len(), 2);
    assert_eq!(config.recent_projects[0].project_path, "/b/project.yaml");
    assert_eq!(config.recent_projects[1].project_path, "/a/project.yaml");
}

#[test]
fn recent_projects_insert_into_empty_list() {
    let mut config = AppConfig::default();
    assert!(config.recent_projects.is_empty());

    insert_recent_project(&mut config, make_entry("/a/project.yaml", "A"));

    assert_eq!(config.recent_projects.len(), 1);
    assert_eq!(config.recent_projects[0].project_name, "A");
}

// ── AppConfig ───────────────────────────────────────────────────────

#[test]
fn app_config_default_empty_recent_projects() {
    let config = AppConfig::default();
    assert!(config.recent_projects.is_empty());
}

#[test]
fn app_config_serde_roundtrip() {
    let config = AppConfig {
        recent_projects: vec![make_entry("/a.yaml", "A"), make_entry("/b.yaml", "B")],
        paths: AssetPaths::default(),
        ..Default::default()
    };
    let yaml = serde_yaml::to_string(&config).unwrap();
    let restored: AppConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(config, restored);
}

#[test]
fn app_config_deserialize_empty_yaml_uses_defaults() {
    let config: AppConfig = serde_yaml::from_str("{}").unwrap();
    assert!(config.recent_projects.is_empty());
    assert_eq!(config.paths, AssetPaths::default());
}

#[test]
fn app_config_midi_mcp_master_switch_default_false() {
    // #712: MIDI and MCP enablement is system config, not a CLI flag.
    // A fresh / legacy config (no keys) must default both subsystems OFF
    // — the packaged app stays quiet until the user opts in.
    let config: AppConfig = serde_yaml::from_str("{}").unwrap();
    assert!(
        !config.midi_enabled,
        "MIDI master switch must default to false"
    );
    assert!(
        !config.mcp_enabled,
        "MCP master switch must default to false"
    );
}

#[test]
fn app_config_midi_mcp_master_switch_roundtrips() {
    // #712: once the user flips the switch it must persist across save/load.
    let config = AppConfig {
        midi_enabled: true,
        mcp_enabled: true,
        ..Default::default()
    };
    let yaml = serde_yaml::to_string(&config).unwrap();
    let restored: AppConfig = serde_yaml::from_str(&yaml).unwrap();
    assert!(restored.midi_enabled);
    assert!(restored.mcp_enabled);
    assert_eq!(config, restored);
}

#[test]
fn app_config_save_and_load_filesystem_roundtrip() {
    let dir = tmp_dir("app_config_roundtrip");
    let path = dir.join("config.yaml");

    let config = AppConfig {
        recent_projects: vec![make_entry("/test/proj.yaml", "TestProj")],
        paths: AssetPaths {
            thumbnails: "my/thumbs".into(),
            ..AssetPaths::default()
        },
        ..Default::default()
    };

    let yaml = serde_yaml::to_string(&config).unwrap();
    fs::write(&path, &yaml).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    let loaded: AppConfig = serde_yaml::from_str(&raw).unwrap();
    assert_eq!(config, loaded);

    let _ = fs::remove_dir_all(&dir);
}

