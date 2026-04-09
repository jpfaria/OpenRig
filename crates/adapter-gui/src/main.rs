#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use adapter_gui::run_desktop_app;
use ui_openrig::{AppRuntimeMode, InteractionMode};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();
    let runtime_mode = match std::env::var("OPENRIG_APP_MODE").ok().as_deref() {
        Some("pedalboard") => AppRuntimeMode::Pedalboard,
        Some("controller") => AppRuntimeMode::Controller,
        Some(x) if x == block_core::EFFECT_TYPE_VST3 => AppRuntimeMode::Vst3Plugin,
        _ => AppRuntimeMode::Standalone,
    };

    let interaction_mode = match std::env::var("OPENRIG_INTERACTION_MODE").ok().as_deref() {
        Some("touch") => InteractionMode::Touch,
        _ => InteractionMode::Mouse,
    };

    let raw_args: Vec<String> = std::env::args().collect();
    let raw_refs: Vec<&str> = raw_args.iter().map(|s| s.as_str()).collect();
    let (cli_project_path, auto_save) = adapter_gui::parse_cli_args_from(&raw_refs);
    run_desktop_app(runtime_mode, interaction_mode, cli_project_path, auto_save)
}
