//! Responsibility: routes the filesystem crate's public surface.

pub mod appearance;
pub mod crash_reporting_config;
pub mod drums_config;
pub mod io_bindings;
pub mod metronome_config;
pub mod midi_device;
pub mod midi_migrate;
pub mod midi_paths;
pub mod midi_profile;
pub mod mixer_config;
pub mod player_config;
pub use appearance::Appearance;
pub mod tone3000_config;
pub use crash_reporting_config::{CrashReportingConfig, CrashReportingProvider};
pub use drums_config::DrumsConfig;
pub use io_bindings::{ChannelMode, IoBinding, IoEndpoint};
pub use metronome_config::MetronomeConfig;
pub use midi_device::{MidiDeviceSelection, MidiPortKey};
pub use mixer_config::MixerStripConfig;
pub use player_config::PlayerConfig;
pub use tone3000_config::Tone3000Config;

#[cfg(test)]
#[path = "midi_profile_tests.rs"]
mod midi_profile_tests;

#[cfg(test)]
#[path = "midi_migrate_tests.rs"]
mod midi_migrate_tests;

pub mod app_config;
pub mod asset_paths;
pub mod config_paths;
pub mod gui_settings;
pub mod legacy_user_data;
pub mod storage;
pub mod user_data_paths;

pub use app_config::{AppConfig, RecentProjectEntry};
pub use asset_paths::{
    asset_paths, bundled_backing_tracks_path, detect_data_root, init_asset_paths,
    resolve_asset_paths, AssetPaths,
};
pub(crate) use gui_settings::LegacyGuiAudioSettings;
pub use gui_settings::{GuiAudioDeviceSettings, GuiSystemSettings};
pub use storage::FilesystemStorage;
pub use user_data_paths::{
    default_backing_tracks_path, default_evaluations_path, default_looper_takes_path,
    default_presets_path, default_projects_path, user_data_root,
};

#[path = "app_config_io.rs"]
mod app_config_io;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lib_settings_tests.rs"]
mod settings_tests;

#[cfg(test)]
#[path = "appearance_tests.rs"]
mod appearance_tests;

#[cfg(test)]
#[path = "tone3000_config_tests.rs"]
mod tone3000_config_tests;

#[cfg(test)]
#[path = "user_data_paths_tests.rs"]
mod user_data_paths_tests;

#[cfg(test)]
#[path = "legacy_user_data_tests.rs"]
mod legacy_user_data_tests;
