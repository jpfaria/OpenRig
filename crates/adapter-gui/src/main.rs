#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use adapter_gui::{init_translations, run_desktop_app};
use infra_filesystem::FilesystemStorage;
use ui_openrig::{AppRuntimeMode, InteractionMode};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    // Load persisted language override (if any) before anything renders.
    // Failures here must not block startup — translations are best-effort.
    let persisted_language = FilesystemStorage::load_gui_audio_settings()
        .ok()
        .flatten()
        .and_then(|s| s.language);
    init_translations(persisted_language.as_deref());

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
    let (arg_project_path, arg_auto_save, arg_fullscreen) = adapter_gui::parse_cli_args_from(&raw_refs);
    let cli_project_path = arg_project_path
        .or_else(|| std::env::var("OPENRIG_PROJECT_PATH").ok().map(std::path::PathBuf::from));
    let auto_save = arg_auto_save
        || std::env::var("OPENRIG_AUTO_SAVE").ok().map_or(false, |v| v == "1" || v.eq_ignore_ascii_case("true"));
    let fullscreen = arg_fullscreen
        || std::env::var("OPENRIG_FULLSCREEN").ok().map_or(false, |v| v == "1" || v.eq_ignore_ascii_case("true"));
    run_desktop_app(runtime_mode, interaction_mode, cli_project_path, auto_save, fullscreen)
}
