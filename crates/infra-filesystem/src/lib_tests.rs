//! Tests for `infra-filesystem`. Lifted from `lib.rs` so the production file
//! stays under the size cap. Re-attached via `#[cfg(test)] #[path] mod tests;`.

use super::*;
use std::fs;
use std::path::PathBuf;

// ── helpers ──────────────────────────────────────────────────────────

/// Create a unique temporary directory for each test.
fn tmp_dir(test_name: &str) -> PathBuf {
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

fn make_device(id: &str, name: &str) -> GuiAudioDeviceSettings {
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
    assert!(!config.midi_enabled, "MIDI master switch must default to false");
    assert!(!config.mcp_enabled, "MCP master switch must default to false");
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

// ── GuiSystemSettings ────────────────────────────────────────────────

#[test]
fn gui_audio_settings_default_empty() {
    let settings = GuiSystemSettings::default();
    assert!(settings.input_devices.is_empty());
    assert!(settings.output_devices.is_empty());
}

#[test]
fn gui_audio_settings_is_complete_both_populated() {
    let settings = GuiSystemSettings {
        input_devices: vec![make_device("in1", "Input 1")],
        output_devices: vec![make_device("out1", "Output 1")],
        language: None,
        midi_devices: vec![],
    };
    assert!(settings.is_complete());
}

#[test]
fn gui_audio_settings_is_complete_missing_input() {
    let settings = GuiSystemSettings {
        input_devices: vec![],
        output_devices: vec![make_device("out1", "Output 1")],
        language: None,
        midi_devices: vec![],
    };
    assert!(!settings.is_complete());
}

#[test]
fn gui_audio_settings_is_complete_missing_output() {
    let settings = GuiSystemSettings {
        input_devices: vec![make_device("in1", "Input 1")],
        output_devices: vec![],
        language: None,
        midi_devices: vec![],
    };
    assert!(!settings.is_complete());
}

#[test]
fn gui_audio_settings_is_complete_both_empty() {
    let settings = GuiSystemSettings::default();
    assert!(!settings.is_complete());
}

#[test]
fn gui_audio_settings_serde_roundtrip() {
    let settings = GuiSystemSettings {
        input_devices: vec![make_device("in1", "Mic"), make_device("in2", "Line In")],
        output_devices: vec![make_device("out1", "Speakers")],
        language: None,
        midi_devices: vec![],
    };
    let yaml = serde_yaml::to_string(&settings).unwrap();
    let restored: GuiSystemSettings = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(settings, restored);
}

#[test]
fn gui_audio_settings_save_and_load_filesystem_roundtrip() {
    let dir = tmp_dir("gui_audio_roundtrip");
    let path = dir.join("gui-settings.yaml");

    let settings = GuiSystemSettings {
        input_devices: vec![make_device("coreaudio:in", "Built-in Mic")],
        output_devices: vec![make_device("coreaudio:out", "Built-in Output")],
        language: None,
        midi_devices: vec![],
    };

    let yaml = serde_yaml::to_string(&settings).unwrap();
    fs::write(&path, &yaml).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    let loaded: GuiSystemSettings = serde_yaml::from_str(&raw).unwrap();
    assert_eq!(settings, loaded);

    let _ = fs::remove_dir_all(&dir);
}

// ── GuiAudioDeviceSettings defaults ─────────────────────────────────

#[test]
fn gui_audio_device_settings_defaults_sample_rate_48000() {
    let yaml = "device_id: x\nname: X\n";
    let dev: GuiAudioDeviceSettings = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(dev.sample_rate, 48_000);
}

#[test]
fn gui_audio_device_settings_defaults_buffer_256() {
    let yaml = "device_id: x\nname: X\n";
    let dev: GuiAudioDeviceSettings = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(dev.buffer_size_frames, 256);
}

#[test]
fn gui_audio_device_settings_defaults_bit_depth_32() {
    let yaml = "device_id: x\nname: X\n";
    let dev: GuiAudioDeviceSettings = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(dev.bit_depth, 32);
}

#[test]
fn gui_audio_device_settings_roundtrip_with_bit_depth() {
    let dev = GuiAudioDeviceSettings {
        device_id: "hw:0".into(),
        name: "Teyun Q26".into(),
        sample_rate: 48_000,
        buffer_size_frames: 64,
        bit_depth: 24,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    };
    let yaml = serde_yaml::to_string(&dev).unwrap();
    let restored: GuiAudioDeviceSettings = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(dev, restored);
    assert_eq!(restored.bit_depth, 24);
}

#[cfg(target_os = "linux")]
#[test]
fn gui_audio_device_settings_defaults_realtime_true() {
    let yaml = "device_id: x\nname: X\n";
    let dev: GuiAudioDeviceSettings = serde_yaml::from_str(yaml).unwrap();
    assert!(dev.realtime);
}

#[cfg(target_os = "linux")]
#[test]
fn gui_audio_device_settings_defaults_rt_priority_70() {
    let yaml = "device_id: x\nname: X\n";
    let dev: GuiAudioDeviceSettings = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(dev.rt_priority, 70);
}

#[cfg(target_os = "linux")]
#[test]
fn gui_audio_device_settings_defaults_nperiods_3() {
    let yaml = "device_id: x\nname: X\n";
    let dev: GuiAudioDeviceSettings = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(dev.nperiods, 3);
}

#[cfg(target_os = "linux")]
#[test]
fn gui_audio_device_settings_roundtrip_with_jack_tuning() {
    let dev = GuiAudioDeviceSettings {
        device_id: "hw:4".into(),
        name: "USB".into(),
        sample_rate: 48_000,
        buffer_size_frames: 64,
        bit_depth: 32,
        realtime: true,
        rt_priority: 80,
        nperiods: 2,
    };
    let yaml = serde_yaml::to_string(&dev).unwrap();
    let restored: GuiAudioDeviceSettings = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(dev, restored);
    assert!(restored.realtime);
    assert_eq!(restored.rt_priority, 80);
    assert_eq!(restored.nperiods, 2);
}

// ── LegacyGuiAudioSettings migration ────────────────────────────────

#[test]
fn legacy_settings_migration_converts_device_names() {
    let yaml = r#"
input_device_names:
  - "Built-in Mic"
output_device_names:
  - "Built-in Output"
sample_rate: 44100
buffer_size_frames: 128
"#;
    let legacy: LegacyGuiAudioSettings = serde_yaml::from_str(yaml).unwrap();
    let modern: GuiSystemSettings = legacy.into();

    assert_eq!(modern.input_devices.len(), 1);
    assert_eq!(modern.input_devices[0].name, "Built-in Mic");
    assert_eq!(modern.input_devices[0].device_id, "");
    assert_eq!(modern.input_devices[0].sample_rate, 44100);
    assert_eq!(modern.input_devices[0].buffer_size_frames, 128);

    assert_eq!(modern.output_devices.len(), 1);
    assert_eq!(modern.output_devices[0].name, "Built-in Output");
    assert_eq!(modern.output_devices[0].sample_rate, 44100);
}

#[test]
fn legacy_settings_migration_empty_lists() {
    let legacy = LegacyGuiAudioSettings::default();
    let modern: GuiSystemSettings = legacy.into();
    assert!(modern.input_devices.is_empty());
    assert!(modern.output_devices.is_empty());
}

#[test]
fn legacy_settings_migration_multiple_devices() {
    let legacy = LegacyGuiAudioSettings {
        input_device_names: vec!["Mic 1".into(), "Mic 2".into()],
        output_device_names: vec!["Out 1".into(), "Out 2".into(), "Out 3".into()],
        sample_rate: 96_000,
        buffer_size_frames: 64,
    };
    let modern: GuiSystemSettings = legacy.into();
    assert_eq!(modern.input_devices.len(), 2);
    assert_eq!(modern.output_devices.len(), 3);
    // All devices share the same sample_rate from legacy
    for dev in &modern.input_devices {
        assert_eq!(dev.sample_rate, 96_000);
        assert_eq!(dev.buffer_size_frames, 64);
    }
}

#[test]
fn app_config_round_trips_midi_devices() {
    let config = AppConfig {
        recent_projects: vec![],
        paths: AssetPaths::default(),
        input_devices: vec![],
        output_devices: vec![],
        language: None,
        midi_devices: vec![MidiDeviceSelection {
            port_key: MidiPortKey {
                name: "Foo".into(),
                instance: 0,
            },
            alias: "Foo".into(),
            enabled: true,
        }],
        ..Default::default()
    };
    let yaml = serde_yaml::to_string(&config).unwrap();
    let back: AppConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back.midi_devices.len(), 1);
    assert_eq!(back.midi_devices[0].alias, "Foo");
}

#[test]
fn legacy_app_config_without_midi_devices_loads_with_empty_list() {
    let yaml = "recent_projects: []\npaths: {}\ninput_devices: []\noutput_devices: []\n";
    let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(config.midi_devices.is_empty());
}

// ── detect_data_root ────────────────────────────────────────────────

#[test]
fn detect_data_root_returns_existing_directory() {
    let root = detect_data_root();
    assert!(root.exists(), "data root should exist: {:?}", root);
    assert!(root.is_dir(), "data root should be a directory: {:?}", root);
}

// ── FilesystemStorage paths ─────────────────────────────────────────

#[test]
fn gui_settings_path_ends_with_expected_filename() {
    let path = FilesystemStorage::gui_settings_path().unwrap();
    assert!(
        path.ends_with("OpenRig/gui-settings.yaml"),
        "unexpected gui settings path: {:?}",
        path
    );
}

#[test]
fn midi_map_path_ends_with_expected_filename() {
    let path = FilesystemStorage::midi_map_path().unwrap();
    assert!(
        path.ends_with("OpenRig/midi-map.yaml"),
        "unexpected midi map path: {:?}",
        path
    );
}

#[test]
fn app_config_path_ends_with_expected_filename() {
    let path = FilesystemStorage::app_config_path().unwrap();
    assert!(
        path.ends_with("OpenRig/config.yaml"),
        "unexpected app config path: {:?}",
        path
    );
}

// ── unified config.yaml — gui-settings.yaml migration (#287) ───────────

#[test]
fn app_config_serdes_unified_audio_and_language_fields() {
    let cfg = AppConfig {
        recent_projects: Vec::new(),
        paths: AssetPaths::default(),
        input_devices: vec![make_device("in1", "Mic 1")],
        output_devices: vec![make_device("out1", "Speakers")],
        language: Some("pt-BR".into()),
        midi_devices: vec![],
        ..Default::default()
    };
    let yaml = serde_yaml::to_string(&cfg).unwrap();
    assert!(yaml.contains("input_devices"));
    assert!(yaml.contains("output_devices"));
    assert!(yaml.contains("language: pt-BR"));
    let restored: AppConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(cfg, restored);
}

#[test]
fn app_config_deserializes_yaml_without_audio_fields() {
    // Older config.yaml predating the unification only had recent_projects.
    let yaml = "recent_projects:\n- project_path: /x\n  project_name: X\n";
    let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(cfg.recent_projects.len(), 1);
    assert!(cfg.input_devices.is_empty());
    assert!(cfg.output_devices.is_empty());
    assert!(cfg.language.is_none());
}

// ── io_bindings in AppConfig (#716) ────────────────────────────────────

#[test]
fn app_config_io_bindings_round_trip() {
    let binding = IoBinding {
        id: "main".into(),
        name: "Scarlett 2i2".into(),
        inputs: vec![IoEndpoint {
            name: "Guitar In 1".into(),
            device_id: "dev-001".into(),
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Monitor Out".into(),
            device_id: "dev-001".into(),
            channels: vec![0, 1],
        }],
    };
    let config = AppConfig {
        io_bindings: vec![binding.clone()],
        ..Default::default()
    };
    let yaml = serde_yaml::to_string(&config).unwrap();
    let back: AppConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back.io_bindings, vec![binding]);
}

#[test]
fn legacy_app_config_without_io_bindings_loads_with_empty_vec() {
    // A minimal legacy config.yaml that predates the io_bindings field.
    let yaml = "recent_projects: []\npaths: {}\ninput_devices: []\noutput_devices: []\n";
    let config: AppConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(
        config.io_bindings.is_empty(),
        "expected empty io_bindings on legacy config, got {:?}",
        config.io_bindings
    );
}
