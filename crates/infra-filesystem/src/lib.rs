//! Responsibility: routes the filesystem crate's public surface.

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
pub use crash_reporting_config::{CrashReportingConfig, CrashReportingProvider};
pub use drums_config::DrumsConfig;
pub use io_bindings::{ChannelMode, IoBinding, IoEndpoint};
pub use metronome_config::MetronomeConfig;
pub use midi_device::{MidiDeviceSelection, MidiPortKey};
pub use mixer_config::MixerStripConfig;
pub use player_config::PlayerConfig;

#[cfg(test)]
#[path = "midi_profile_tests.rs"]
mod midi_profile_tests;

#[cfg(test)]
#[path = "midi_migrate_tests.rs"]
mod midi_migrate_tests;

pub mod app_config;
pub mod asset_paths;
#[cfg(any(target_os = "windows", test))]
mod config_base;
pub mod config_paths;
pub mod gui_settings;
#[cfg(any(target_os = "windows", test))]
mod install_root;
pub mod storage;

pub use app_config::{AppConfig, RecentProjectEntry};
pub use asset_paths::{
    asset_paths, bundled_backing_tracks_path, default_backing_tracks_path,
    default_evaluations_path, default_looper_takes_path, detect_data_root, init_asset_paths,
    resolve_asset_paths, user_data_root, AssetPaths,
};
pub(crate) use gui_settings::LegacyGuiAudioSettings;
pub use gui_settings::{GuiAudioDeviceSettings, GuiSystemSettings};
pub use storage::FilesystemStorage;

#[path = "app_config_io.rs"]
mod app_config_io;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lib_settings_tests.rs"]
mod settings_tests;
