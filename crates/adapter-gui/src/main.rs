#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use adapter_gui::render_dispatch::{classify_launch, LaunchMode};
use adapter_gui::{init_translations, run_desktop_app};
use infra_filesystem::FilesystemStorage;
use ui_openrig::{AppRuntimeMode, InteractionMode};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // `--render` short-circuits to the headless offline renderer (issue
    // #552). Mutually exclusive with `--mcp`/`--midi`/positional project
    // path — the classifier rejects those before we touch any Slint or
    // engine init code. Exits 2 on classifier error, 1 on render error.
    let raw_argv: Vec<String> = std::env::args().collect();
    match classify_launch(&raw_argv) {
        Ok(LaunchMode::Render(args)) => {
            return match adapter_render::render(&args) {
                Ok(summary) => {
                    log::info!(
                        "openrig --render: wrote {} frames at {} Hz → {}",
                        summary.frames_written,
                        summary.sample_rate_hz,
                        summary.output.display()
                    );
                    Ok(())
                }
                Err(e) => {
                    eprintln!("openrig --render: {e}");
                    std::process::exit(1);
                }
            };
        }
        Ok(LaunchMode::Gui) => {}
        Err(e) => {
            eprintln!("openrig: {e}");
            std::process::exit(2);
        }
    }

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
    let (arg_project_path, arg_auto_save, arg_fullscreen) =
        adapter_gui::parse_cli_args_from(&raw_refs);
    let mcp_addr = adapter_gui::parse_mcp_addr(&raw_refs);
    let midi_map = adapter_gui::parse_midi_map(&raw_refs);
    let cli_project_path = arg_project_path
        .or_else(|| {
            std::env::var("OPENRIG_PROJECT_PATH")
                .ok()
                .map(std::path::PathBuf::from)
        })
        .and_then(|path| {
            // #452: validate the resolved path with a clear message and fall
            // back to the launcher (no crash) when it is bad — per the
            // cli-project-path-autosave spec.
            match adapter_gui::validate_project_path(&path) {
                Ok(()) => Some(path),
                Err(e) => {
                    eprintln!("openrig: {e}; opening launcher instead");
                    None
                }
            }
        });
    let auto_save = arg_auto_save
        || std::env::var("OPENRIG_AUTO_SAVE")
            .ok()
            .map_or(false, |v| v == "1" || v.eq_ignore_ascii_case("true"));
    let fullscreen = arg_fullscreen
        || std::env::var("OPENRIG_FULLSCREEN")
            .ok()
            .map_or(false, |v| v == "1" || v.eq_ignore_ascii_case("true"));
    run_desktop_app(
        runtime_mode,
        interaction_mode,
        cli_project_path,
        auto_save,
        fullscreen,
        mcp_addr,
        midi_map,
    )
}
