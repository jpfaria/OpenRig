use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Central configuration for all asset directories.
///
/// Each field holds a path (absolute or relative to the executable) where the
/// corresponding asset category lives.  When the app starts it loads these
/// values from `config.yaml` (falling back to sensible per-platform defaults)
/// and stores them in a global `OnceLock` so every crate can access them
/// without passing config around.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPaths {
    /// Directory containing prebuilt LV2 shared libraries (.dylib/.so/.dll).
    #[serde(default = "default_lv2_libs")]
    pub lv2_libs: String,
    /// Directory containing LV2 plugin data (TTL metadata, presets).
    #[serde(default = "default_lv2_data")]
    pub lv2_data: String,
    /// Root directory for NAM capture files (.nam).
    #[serde(default = "default_nam_captures")]
    pub nam_captures: String,
    /// Root directory for IR capture files (.wav).
    #[serde(default = "default_ir_captures")]
    pub ir_captures: String,
    /// Root directory for block thumbnails (PNG images).
    #[serde(default = "default_thumbnails")]
    pub thumbnails: String,
    /// Root directory for block screenshots (PNG images for info panel).
    #[serde(default = "default_screenshots")]
    pub screenshots: String,
    /// Root directory for plugin metadata YAML files (per-language).
    #[serde(default = "default_metadata")]
    pub metadata: String,
}

impl Default for AssetPaths {
    fn default() -> Self {
        Self {
            lv2_libs: default_lv2_libs(),
            lv2_data: default_lv2_data(),
            nam_captures: default_nam_captures(),
            ir_captures: default_ir_captures(),
            thumbnails: default_thumbnails(),
            screenshots: default_screenshots(),
            metadata: default_metadata(),
        }
    }
}

fn default_lv2_libs() -> String {
    #[cfg(target_os = "macos")]
    { "libs/lv2/macos-universal".to_string() }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { "libs/lv2/linux-x86_64".to_string() }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    { "libs/lv2/linux-aarch64".to_string() }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { "libs/lv2/windows-x64".to_string() }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    { "libs/lv2/windows-arm64".to_string() }
}

fn default_lv2_data() -> String {
    "data/lv2".to_string()
}

fn default_nam_captures() -> String {
    "captures/nam".to_string()
}

fn default_ir_captures() -> String {
    "captures/ir".to_string()
}

fn default_thumbnails() -> String {
    "assets/blocks/thumbnails".to_string()
}

fn default_screenshots() -> String {
    "assets/blocks/screenshots".to_string()
}

fn default_metadata() -> String {
    "assets/blocks/metadata".to_string()
}

static ASSET_PATHS: OnceLock<AssetPaths> = OnceLock::new();

/// Detect the application data root for the current installation layout.
///
/// Returns the directory that contains `libs/`, `data/`, and `assets/`:
///
/// - macOS `.app` bundle: `<bundle>/Contents/Resources/`
/// - Linux deb/rpm: `/usr/share/openrig/`
/// - Windows MSI: directory alongside the executable
/// - Development fallback: current working directory
pub fn detect_data_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        #[cfg(target_os = "macos")]
        if let Some(resources) = exe
            .parent() // .app/Contents/MacOS/
            .and_then(|p| p.parent()) // .app/Contents/
            .map(|p| p.join("Resources"))
        {
            if resources.exists() {
                return resources;
            }
        }

        #[cfg(target_os = "linux")]
        if let Some(exe_dir) = exe.parent() {
            if let Some(prefix) = exe_dir.parent() {
                let share = prefix.join("share/openrig");
                if share.exists() {
                    return share;
                }
            }
        }

        #[cfg(target_os = "windows")]
        if let Some(exe_dir) = exe.parent() {
            if exe_dir.join("libs").exists() {
                return exe_dir.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Resolve relative asset paths against the detected data root.
///
/// Absolute paths in `paths` are left unchanged. Relative paths are joined
/// with `detect_data_root()` so the app finds its assets regardless of the
/// current working directory.
pub fn resolve_asset_paths(paths: AssetPaths) -> AssetPaths {
    let root = detect_data_root();
    fn resolve(root: &std::path::Path, s: String) -> String {
        let p = std::path::Path::new(&s);
        if p.is_absolute() { s } else { root.join(p).to_string_lossy().into_owned() }
    }
    AssetPaths {
        lv2_libs: resolve(&root, paths.lv2_libs),
        lv2_data: resolve(&root, paths.lv2_data),
        nam_captures: resolve(&root, paths.nam_captures),
        ir_captures: resolve(&root, paths.ir_captures),
        thumbnails: resolve(&root, paths.thumbnails),
        screenshots: resolve(&root, paths.screenshots),
        metadata: resolve(&root, paths.metadata),
    }
}

/// Store the resolved asset paths for the lifetime of the process.
///
/// Must be called once during app startup (after loading config).  Subsequent
/// calls are silently ignored so that tests that initialise multiple times do
/// not panic.
pub fn init_asset_paths(paths: AssetPaths) {
    ASSET_PATHS.set(paths).ok();
}

/// Retrieve the global asset paths.
///
/// # Panics
/// Panics if `init_asset_paths` has not been called yet.
pub fn asset_paths() -> &'static AssetPaths {
    ASSET_PATHS.get().expect("asset_paths not initialized — call init_asset_paths() during startup")
}

pub struct FilesystemStorage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecentProjectEntry {
    pub project_path: String,
    pub project_name: String,
    #[serde(default = "default_true")]
    pub is_valid: bool,
    #[serde(default)]
    pub invalid_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub recent_projects: Vec<RecentProjectEntry>,
    #[serde(default)]
    pub paths: AssetPaths,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiAudioDeviceSettings {
    pub device_id: String,
    pub name: String,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_buffer_size_frames")]
    pub buffer_size_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiAudioSettings {
    #[serde(default)]
    pub input_devices: Vec<GuiAudioDeviceSettings>,
    #[serde(default)]
    pub output_devices: Vec<GuiAudioDeviceSettings>,
}

impl GuiAudioSettings {
    pub fn is_complete(&self) -> bool {
        !self.input_devices.is_empty() && !self.output_devices.is_empty()
    }
}

fn default_sample_rate() -> u32 {
    48_000
}

fn default_buffer_size_frames() -> u32 {
    256
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct LegacyGuiAudioSettings {
    #[serde(default)]
    input_device_names: Vec<String>,
    #[serde(default)]
    output_device_names: Vec<String>,
    #[serde(default = "default_sample_rate")]
    sample_rate: u32,
    #[serde(default = "default_buffer_size_frames")]
    buffer_size_frames: u32,
}

impl From<LegacyGuiAudioSettings> for GuiAudioSettings {
    fn from(value: LegacyGuiAudioSettings) -> Self {
        let input_devices = value
            .input_device_names
            .into_iter()
            .map(|name| GuiAudioDeviceSettings {
                device_id: String::new(),
                name,
                sample_rate: value.sample_rate,
                buffer_size_frames: value.buffer_size_frames,
            })
            .collect();
        let output_devices = value
            .output_device_names
            .into_iter()
            .map(|name| GuiAudioDeviceSettings {
                device_id: String::new(),
                name,
                sample_rate: value.sample_rate,
                buffer_size_frames: value.buffer_size_frames,
            })
            .collect();
        Self {
            input_devices,
            output_devices,
        }
    }
}

impl FilesystemStorage {
    pub fn gui_settings_path() -> Result<PathBuf> {
        let base_dir = dirs::config_dir()
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("gui-settings.yaml"))
    }

    pub fn app_config_path() -> Result<PathBuf> {
        let base_dir = dirs::config_dir()
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("config.yaml"))
    }

    pub fn load_gui_audio_settings() -> Result<Option<GuiAudioSettings>> {
        let path = Self::gui_settings_path()?;
        log::info!("loading gui audio settings from {:?}", path);
        if !path.exists() {
            log::debug!("gui audio settings file not found, returning None");
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read gui settings from {:?}", path))?;
        let settings = match serde_yaml::from_str::<GuiAudioSettings>(&raw) {
            Ok(settings) => settings,
            Err(_) => {
                log::warn!("failed to parse gui settings as current format, trying legacy format");
                let legacy = serde_yaml::from_str::<LegacyGuiAudioSettings>(&raw)
                    .with_context(|| format!("failed to parse gui settings from {:?}", path))?;
                legacy.into()
            }
        };
        Ok(Some(settings))
    }

    pub fn save_gui_audio_settings(settings: &GuiAudioSettings) -> Result<()> {
        let path = Self::gui_settings_path()?;
        log::info!("saving gui audio settings to {:?}", path);
        let parent = path
            .parent()
            .context("gui settings path has no parent directory")?;
        log::debug!("ensuring directory exists: {:?}", parent);
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create gui settings directory {:?}", parent))?;
        let raw = serde_yaml::to_string(settings)?;
        fs::write(&path, raw)
            .with_context(|| format!("failed to write gui settings to {:?}", path))?;
        Ok(())
    }

    pub fn load_app_config() -> Result<AppConfig> {
        let path = Self::app_config_path()?;
        log::info!("loading app config from {:?}", path);
        if !path.exists() {
            log::debug!("app config file not found, using defaults");
            return Ok(AppConfig::default());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read app config from {:?}", path))?;
        serde_yaml::from_str(&raw)
            .with_context(|| format!("failed to parse app config from {:?}", path))
    }

    pub fn save_app_config(config: &AppConfig) -> Result<()> {
        let path = Self::app_config_path()?;
        log::info!("saving app config to {:?}", path);
        let parent = path
            .parent()
            .context("app config path has no parent directory")?;
        log::debug!("ensuring directory exists: {:?}", parent);
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create app config directory {:?}", parent))?;
        let raw = serde_yaml::to_string(config)?;
        fs::write(&path, raw)
            .with_context(|| format!("failed to write app config to {:?}", path))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
        }
    }

    // ── AssetPaths ──────────────────────────────────────────────────────

    #[test]
    fn asset_paths_default_has_platform_lv2_libs() {
        let paths = AssetPaths::default();
        assert!(
            paths.lv2_libs.starts_with("libs/lv2/"),
            "lv2_libs should start with libs/lv2/, got: {}",
            paths.lv2_libs
        );
    }

    #[test]
    fn asset_paths_default_fields_not_empty() {
        let paths = AssetPaths::default();
        assert!(!paths.lv2_libs.is_empty());
        assert!(!paths.lv2_data.is_empty());
        assert!(!paths.nam_captures.is_empty());
        assert!(!paths.ir_captures.is_empty());
        assert!(!paths.thumbnails.is_empty());
        assert!(!paths.screenshots.is_empty());
        assert!(!paths.metadata.is_empty());
    }

    #[test]
    fn asset_paths_serde_roundtrip_preserves_values() {
        let paths = AssetPaths {
            lv2_libs: "custom/lv2/libs".into(),
            lv2_data: "custom/lv2/data".into(),
            nam_captures: "custom/nam".into(),
            ir_captures: "custom/ir".into(),
            thumbnails: "custom/thumbs".into(),
            screenshots: "custom/screens".into(),
            metadata: "custom/meta".into(),
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
            lv2_libs: "/absolute/lv2/libs".into(),
            lv2_data: "/absolute/lv2/data".into(),
            nam_captures: "/absolute/nam".into(),
            ir_captures: "/absolute/ir".into(),
            thumbnails: "/absolute/thumbs".into(),
            screenshots: "/absolute/screens".into(),
            metadata: "/absolute/meta".into(),
        };
        let resolved = resolve_asset_paths(paths.clone());
        assert_eq!(resolved.lv2_libs, "/absolute/lv2/libs");
        assert_eq!(resolved.lv2_data, "/absolute/lv2/data");
        assert_eq!(resolved.nam_captures, "/absolute/nam");
        assert_eq!(resolved.ir_captures, "/absolute/ir");
        assert_eq!(resolved.thumbnails, "/absolute/thumbs");
        assert_eq!(resolved.screenshots, "/absolute/screens");
        assert_eq!(resolved.metadata, "/absolute/meta");
    }

    #[test]
    fn resolve_asset_paths_relative_gets_root_prepended() {
        let paths = AssetPaths::default();
        let resolved = resolve_asset_paths(paths.clone());
        // Relative paths should become absolute after resolution
        assert!(
            std::path::Path::new(&resolved.lv2_libs).is_absolute()
                || resolved.lv2_libs.contains('/'),
            "resolved lv2_libs should have root prepended: {}",
            resolved.lv2_libs
        );
        // The resolved path should end with the original relative path
        assert!(
            resolved.lv2_libs.ends_with(&paths.lv2_libs),
            "resolved path '{}' should end with '{}'",
            resolved.lv2_libs,
            paths.lv2_libs
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
            recent_projects: vec![
                make_entry("/a.yaml", "A"),
                make_entry("/b.yaml", "B"),
            ],
            paths: AssetPaths::default(),
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
    fn app_config_save_and_load_filesystem_roundtrip() {
        let dir = tmp_dir("app_config_roundtrip");
        let path = dir.join("config.yaml");

        let config = AppConfig {
            recent_projects: vec![make_entry("/test/proj.yaml", "TestProj")],
            paths: AssetPaths {
                lv2_libs: "my/lv2".into(),
                ..AssetPaths::default()
            },
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        fs::write(&path, &yaml).unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        let loaded: AppConfig = serde_yaml::from_str(&raw).unwrap();
        assert_eq!(config, loaded);

        let _ = fs::remove_dir_all(&dir);
    }

    // ── GuiAudioSettings ────────────────────────────────────────────────

    #[test]
    fn gui_audio_settings_default_empty() {
        let settings = GuiAudioSettings::default();
        assert!(settings.input_devices.is_empty());
        assert!(settings.output_devices.is_empty());
    }

    #[test]
    fn gui_audio_settings_is_complete_both_populated() {
        let settings = GuiAudioSettings {
            input_devices: vec![make_device("in1", "Input 1")],
            output_devices: vec![make_device("out1", "Output 1")],
        };
        assert!(settings.is_complete());
    }

    #[test]
    fn gui_audio_settings_is_complete_missing_input() {
        let settings = GuiAudioSettings {
            input_devices: vec![],
            output_devices: vec![make_device("out1", "Output 1")],
        };
        assert!(!settings.is_complete());
    }

    #[test]
    fn gui_audio_settings_is_complete_missing_output() {
        let settings = GuiAudioSettings {
            input_devices: vec![make_device("in1", "Input 1")],
            output_devices: vec![],
        };
        assert!(!settings.is_complete());
    }

    #[test]
    fn gui_audio_settings_is_complete_both_empty() {
        let settings = GuiAudioSettings::default();
        assert!(!settings.is_complete());
    }

    #[test]
    fn gui_audio_settings_serde_roundtrip() {
        let settings = GuiAudioSettings {
            input_devices: vec![
                make_device("in1", "Mic"),
                make_device("in2", "Line In"),
            ],
            output_devices: vec![make_device("out1", "Speakers")],
        };
        let yaml = serde_yaml::to_string(&settings).unwrap();
        let restored: GuiAudioSettings = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(settings, restored);
    }

    #[test]
    fn gui_audio_settings_save_and_load_filesystem_roundtrip() {
        let dir = tmp_dir("gui_audio_roundtrip");
        let path = dir.join("gui-settings.yaml");

        let settings = GuiAudioSettings {
            input_devices: vec![make_device("coreaudio:in", "Built-in Mic")],
            output_devices: vec![make_device("coreaudio:out", "Built-in Output")],
        };

        let yaml = serde_yaml::to_string(&settings).unwrap();
        fs::write(&path, &yaml).unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        let loaded: GuiAudioSettings = serde_yaml::from_str(&raw).unwrap();
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
        let modern: GuiAudioSettings = legacy.into();

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
        let modern: GuiAudioSettings = legacy.into();
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
        let modern: GuiAudioSettings = legacy.into();
        assert_eq!(modern.input_devices.len(), 2);
        assert_eq!(modern.output_devices.len(), 3);
        // All devices share the same sample_rate from legacy
        for dev in &modern.input_devices {
            assert_eq!(dev.sample_rate, 96_000);
            assert_eq!(dev.buffer_size_frames, 64);
        }
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
    fn app_config_path_ends_with_expected_filename() {
        let path = FilesystemStorage::app_config_path().unwrap();
        assert!(
            path.ends_with("OpenRig/config.yaml"),
            "unexpected app config path: {:?}",
            path
        );
    }
}
