mod thumbnails;
mod plugin_info;
mod app;
pub(crate) mod callbacks;
mod audio_devices;
mod block_editor;
mod chain_editor;
mod eq;
mod helpers;
mod io_groups;
mod project_ops;
mod project_view;
mod state;
mod ui_state;
mod visual_config;

slint::include_modules!();

pub use app::parse_cli_args_from;
pub use app::run_desktop_app;
pub(crate) use app::ui_index_to_real_block_index;
pub(crate) use app::sync_live_chain_runtime;

pub(crate) const SELECT_PATH_PREFIX: &str = "__select.";
pub(crate) const SELECT_SELECTED_BLOCK_ID: &str = "__select.selected_block_id";
pub(crate) const DEFAULT_SAMPLE_RATE: u32 = 48_000;
pub(crate) const DEFAULT_BUFFER_SIZE_FRAMES: u32 = 64;
pub(crate) const DEFAULT_BIT_DEPTH: u32 = 32;
pub(crate) const SUPPORTED_SAMPLE_RATES: &[u32] = &[44_100, 48_000, 88_200, 96_000];
pub(crate) const SUPPORTED_BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];
pub(crate) const SUPPORTED_BIT_DEPTHS: &[u32] = &[16, 24, 32];

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
