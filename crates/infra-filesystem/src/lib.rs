//! Responsibility: routes the filesystem crate's public surface.

pub mod io_bindings;
pub mod metronome_config;
pub mod midi_device;
pub mod midi_migrate;
pub mod midi_paths;
pub mod midi_profile;
pub use io_bindings::{ChannelMode, IoBinding, IoEndpoint};
pub use metronome_config::MetronomeConfig;
pub use midi_device::{MidiDeviceSelection, MidiPortKey};

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
pub mod storage;

pub use app_config::{AppConfig, RecentProjectEntry};
pub use asset_paths::{
    asset_paths, default_evaluations_path, detect_data_root, init_asset_paths, resolve_asset_paths,
    user_data_root, AssetPaths,
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
