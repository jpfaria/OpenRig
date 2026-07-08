// Snapshot of complexity debt that existed on develop before the
// #548 build break was fixed (issue #576). Refactor of long fns and
// complex types is tracked under god-file ticket #276 and follow-ups.
// Allowing crate-wide keeps the QG honest about NEW regressions
// instead of perpetually re-reporting the existing snapshot.
#![allow(clippy::too_many_lines)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use adapter_gui::{init_translations, run_desktop_app};
use infra_filesystem::FilesystemStorage;
use ui_openrig::{AppRuntimeMode, InteractionMode};

fn main() -> anyhow::Result<()> {
    // #693: non-blocking logger — log calls must never stall the GUI
    // thread on a slow stderr consumer.
    adapter_gui::logging::init_logging();

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
    // #712: MIDI/MCP enablement is per-machine config (config.yaml), not a
    // launch flag — so packaged builds (which start the binary with no args)
    // can enable them. The CLI `--midi`/`--mcp` flags stay as a dev override
    // that wins when present. Config-load failure → treat as disabled (the
    // flags still work), never block startup.
    let app_config = FilesystemStorage::load_app_config().unwrap_or_default();
    let mcp_addr =
        adapter_gui::resolve_mcp_addr(adapter_gui::parse_mcp_addr(&raw_refs), app_config.mcp_enabled);
    let midi_map = adapter_gui::resolve_midi_map(
        adapter_gui::parse_midi_map(&raw_refs),
        app_config.midi_enabled,
    );
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
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let fullscreen = arg_fullscreen
        || std::env::var("OPENRIG_FULLSCREEN")
            .ok()
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let result = run_desktop_app(
        runtime_mode,
        interaction_mode,
        cli_project_path,
        auto_save,
        fullscreen,
        mcp_addr,
        midi_map,
    );
    // #693: saves are queued to the persist worker — wait for
    // durability before the process exits.
    application::persist_worker::flush();
    result
}
