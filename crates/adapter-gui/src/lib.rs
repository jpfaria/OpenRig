mod thumbnails;
mod plugin_info;
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
mod app;

slint::include_modules!();

const SELECT_PATH_PREFIX: &str = "__select.";
const SELECT_SELECTED_BLOCK_ID: &str = "__select.selected_block_id";
const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const DEFAULT_BUFFER_SIZE_FRAMES: u32 = 64;
const DEFAULT_BIT_DEPTH: u32 = 32;
const SUPPORTED_SAMPLE_RATES: &[u32] = &[44_100, 48_000, 88_200, 96_000];
const SUPPORTED_BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];
const SUPPORTED_BIT_DEPTHS: &[u32] = &[16, 24, 32];

pub use app::run_desktop_app;

use std::path::PathBuf;

pub fn parse_cli_args_from(args: &[&str]) -> (Option<PathBuf>, bool, bool) {
    let mut project_path: Option<PathBuf> = None;
    let mut auto_save = false;
    let mut fullscreen = false;
    for arg in args.iter().skip(1) {
        if *arg == "--auto-save" {
            auto_save = true;
        } else if *arg == "--fullscreen" {
            fullscreen = true;
        } else if !arg.starts_with('-') {
            project_path = Some(PathBuf::from(arg));
        }
    }
    (project_path, auto_save, fullscreen)
}
