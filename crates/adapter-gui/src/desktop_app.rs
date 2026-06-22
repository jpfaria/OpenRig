//! `pub fn run_desktop_app` — desktop binary entry point and the place
//! where every wiring module gets stitched together.
//!
//! Lifted out of `lib.rs` so the crate root finally collapses to a thin
//! mod-declarations + re-exports shell. The body below is a long sequence
//! of state initialization (`Rc<RefCell<...>>` handles, Slint VecModels,
//! AppWindow + child windows) followed by ~50 `*_wiring::wire(...)` calls
//! that hand each callback group its own focused module.
//!
//! The function is intentionally kept as a single fn rather than further
//! decomposed: its job is orchestration, and breaking the wiring sequence
//! into helpers would add per-helper Ctx structs while obscuring the
//! linear startup order that the comments here document.

use anyhow::{anyhow, Result};
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use infra_filesystem::FilesystemStorage;
use slint::{ComponentHandle, ModelRc, Timer, VecModel};
use std::cell::{Cell, RefCell};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::rc::Rc;
use ui_openrig::{AppRuntimeMode, InteractionMode, UiRuntimeContext};

use crate::audio_devices::{build_device_selection_items, mark_unselected_devices};
use crate::project_ops::{load_and_sync_app_config, resolve_project_paths};
use crate::state::{
    AudioSettingsMode, BlockEditorDraft, BlockWindow, ChainDraft, InsertDraft, IoBlockInsertDraft,
    ProjectSession, SelectedBlock,
};
use crate::{
    latency_probe, AppWindow, BlockEditorWindow, ChainEditorWindow, ChainInputGroupsWindow,
    ChainInputWindow, ChainInsertWindow, ChainOutputGroupsWindow, ChainOutputWindow,
    ChannelOptionItem, CompactChainViewWindow, PluginInfoWindow, ProjectSettingsWindow,
    SpectrumWindow, TunerWindow,
};

pub fn run_desktop_app(
    runtime_mode: AppRuntimeMode,
    interaction_mode: InteractionMode,
    cli_project_path: Option<PathBuf>,
    auto_save: bool,
    fullscreen: bool,
    mcp_addr: Option<SocketAddr>,
    midi_map: Option<crate::cli::MidiMapArg>,
) -> Result<()> {
    log::info!(
        "starting desktop app: runtime_mode={:?}, interaction_mode={:?}",
        runtime_mode,
        interaction_mode
    );
    // #693 diagnostic: UI event-loop watchdog. A background thread posts a
    // heartbeat into the Slint event loop every 250 ms; if the loop stops
    // answering past ~600 ms a `[ui-stall]` warn names the freeze with its
    // duration — real-session stalls become self-reporting.
    //
    // #721: a large gap alone is ambiguous. A genuine freeze leaves the rest
    // of the process (this thread included) running, so the watchdog keeps
    // waking on schedule while the UI gap grows. An OS pause (macOS App Nap /
    // display sleep / timer coalescing on an idle background app) parks the
    // whole process, so this thread is frozen too and its own wake interval
    // balloons in lockstep with the gap. `is_genuine_ui_stall` warns only in
    // the former case, so idle/backgrounded pauses no longer cry wolf.
    {
        use crate::ui_stall::is_genuine_ui_stall;
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        const TICK: Duration = Duration::from_millis(250);
        const THRESHOLD: Duration = Duration::from_millis(600);

        let beat = Arc::new(AtomicU64::new(0));
        let beat_bg = Arc::clone(&beat);
        std::thread::Builder::new()
            .name("ui-watchdog".into())
            .spawn(move || {
                let start = Instant::now();
                let mut warned_at: u64 = 0;
                let mut last_wake = Instant::now();
                loop {
                    std::thread::sleep(TICK);
                    // How long this thread was actually away: ~TICK under
                    // normal scheduling, but it balloons toward the UI gap when
                    // the OS parked the whole process.
                    let now = Instant::now();
                    let wake_interval = now.duration_since(last_wake);
                    last_wake = now;

                    let now_ms = start.elapsed().as_millis() as u64;
                    let seen = beat_bg.load(Ordering::Relaxed);
                    let gap = Duration::from_millis(now_ms.saturating_sub(seen));
                    if seen > 0
                        && seen != warned_at
                        && is_genuine_ui_stall(gap, wake_interval, TICK, THRESHOLD)
                    {
                        log::warn!(
                            "[ui-stall] event loop unresponsive for ~{}ms",
                            gap.as_millis()
                        );
                        warned_at = seen;
                    }
                    let beat_ui = Arc::clone(&beat_bg);
                    let _ = slint::invoke_from_event_loop(move || {
                        // Runs ON the event loop: the stored instant is the
                        // moment the loop actually answered.
                        beat_ui.store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
                    });
                }
            })
            .ok();
    }
    let context = UiRuntimeContext::new(runtime_mode, interaction_mode);
    let settings = FilesystemStorage::load_gui_audio_settings()?.unwrap_or_default();
    let needs_audio_settings =
        context.capabilities.can_select_audio_device && !settings.is_complete();
    let project_paths = resolve_project_paths();
    let loaded_config = load_and_sync_app_config()?;
    let resolved_paths = infra_filesystem::resolve_asset_paths(loaded_config.paths.clone());
    infra_filesystem::init_asset_paths(resolved_paths);
    // Discover every plugin package shipped under the configured roots
    // and cache them process-wide. Two roots are scanned by default:
    //   1. Bundled (read-only, ships with the installer): under
    //      detect_data_root()/plugins. Has priority — when the same
    //      package id exists in both roots, this one wins.
    //   2. User-installed (writable): plugins_root_from_config(), which
    //      defaults to <config_dir>/plugins next to the GUI config file.
    // Block-* crates query the merged catalog to surface plugin
    // manifests in the GUI.
    // #693: warm the device cache off-thread so the first project open /
    // IO window never pays the ~2s CoreAudio enumeration on the GUI
    // thread (measured: the [ui-stall] 760ms at boot + the open delay).
    std::thread::Builder::new()
        .name("device-cache-warmer".into())
        .spawn(|| {
            let _ = infra_cpal::list_input_device_descriptors();
            let _ = infra_cpal::list_output_device_descriptors();
        })
        .ok();
    let bundled_root = infra_filesystem::detect_data_root().join("plugins");
    let user_root = plugin_loader::plugins_root_from_config(&project_paths.default_config_path);
    log::info!(
        "scanning plugin roots: bundled={} user={}",
        bundled_root.display(),
        user_root.display(),
    );
    // Native plugins (compiled-in DSP) register first; disk-package
    // discovery in `init_many` below pushes its results into the same
    // catalog, so by the time `packages()` is read everything lives in
    // one place.
    engine::native_registry::register_all_natives();
    plugin_loader::registry::init_many(&[bundled_root, user_root]);
    log::info!(
        "plugin catalog ready: {} plugin(s) loaded ({} native, {} disk package(s))",
        plugin_loader::registry::len(),
        plugin_loader::registry::native_count(),
        plugin_loader::registry::len() - plugin_loader::registry::native_count(),
    );
    // Open VST3 editor handles (kept alive so the OS window stays open).
    let vst3_editor_handles: Rc<RefCell<Vec<Box<dyn project::vst3_editor::PluginEditorHandle>>>> =
        Rc::new(RefCell::new(Vec::new()));
    let vst3_editor_handles_for_on_open = vst3_editor_handles.clone();
    // Scan system VST3 paths in a background thread so startup isn't blocked.
    // The catalog is available before any project is opened.
    let vst3_sample_rate = settings
        .input_devices
        .first()
        .map(|d| d.sample_rate)
        .unwrap_or(48_000) as f64;
    std::thread::spawn(move || {
        project::vst3_editor::init_vst3_catalog(vst3_sample_rate);
    });
    let app_config = Rc::new(RefCell::new(loaded_config));
    let project_session = Rc::new(RefCell::new(None::<ProjectSession>));
    let chain_draft = Rc::new(RefCell::new(None::<ChainDraft>));
    let io_block_insert_draft = Rc::new(RefCell::new(None::<IoBlockInsertDraft>));
    let insert_draft = Rc::new(RefCell::new(None::<InsertDraft>));
    let selected_block = Rc::new(RefCell::new(None::<SelectedBlock>));
    let block_editor_draft = Rc::new(RefCell::new(None::<BlockEditorDraft>));
    let project_runtime = Rc::new(RefCell::new(None::<ProjectRuntimeController>));
    let probe_windows = latency_probe::new_windows();
    let saved_project_snapshot = Rc::new(RefCell::new(None::<String>));
    let project_dirty = Rc::new(RefCell::new(false));
    let open_block_windows: Rc<RefCell<Vec<BlockWindow>>> = Rc::new(RefCell::new(Vec::new()));
    let inline_stream_timer: Rc<RefCell<Option<Timer>>> = Rc::new(RefCell::new(None));
    let open_compact_window: Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>> =
        Rc::new(RefCell::new(None));
    let audio_settings_mode = Rc::new(RefCell::new(AudioSettingsMode::Gui));
    // Start with empty device descriptors. Enumerating here would read
    // /proc/asound/cards (and transitively /proc/asound/card*/stream0), which
    // invokes the kernel snd-usb-audio proc handler and has been correlated
    // with vendor-firmware notifications that destabilize USB audio interfaces
    // on fragile xHCI controllers. Descriptors are populated lazily by
    // refresh_input_devices / refresh_output_devices when the user actually
    // opens a chain I/O editor or the Settings panel — i.e. when they
    // explicitly ask the app to look at the hardware.
    let input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>> =
        Rc::new(RefCell::new(Vec::new()));
    let output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>> =
        Rc::new(RefCell::new(Vec::new()));
    let preset_file_list: Rc<RefCell<Vec<std::path::PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
    let window = AppWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let boot_font = crate::i18n::font_for_persisted_runtime();
    {
        use slint::Global;
        crate::Locale::get(&window).set_font_family(boot_font.into());
    }
    // Slint's select_bundled_translation requires at least one component to
    // exist before it can resolve the bundled language list. Call it here,
    // after AppWindow is constructed.
    let persisted_language = FilesystemStorage::load_gui_audio_settings()
        .ok()
        .flatten()
        .and_then(|s| s.language);
    crate::i18n::apply_bundled_translation(persisted_language.as_deref());
    window
        .window()
        .set_size(slint::WindowSize::Logical(slint::LogicalSize {
            width: 1100.0,
            height: 620.0,
        }));
    let project_settings_window =
        ProjectSettingsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    {
        use slint::Global;
        crate::Locale::get(&project_settings_window).set_font_family(boot_font.into());
    }
    let chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>> = Rc::new(RefCell::new(None));
    let plugin_info_window: Rc<RefCell<Option<PluginInfoWindow>>> = Rc::new(RefCell::new(None));
    let chain_input_window = ChainInputWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    {
        use slint::Global;
        crate::Locale::get(&chain_input_window).set_font_family(boot_font.into());
    }
    let chain_output_window =
        ChainOutputWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    {
        use slint::Global;
        crate::Locale::get(&chain_output_window).set_font_family(boot_font.into());
    }
    let chain_input_groups_window =
        ChainInputGroupsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    {
        use slint::Global;
        crate::Locale::get(&chain_input_groups_window).set_font_family(boot_font.into());
    }
    let chain_output_groups_window =
        ChainOutputGroupsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    {
        use slint::Global;
        crate::Locale::get(&chain_output_groups_window).set_font_family(boot_font.into());
    }
    // Tracks whether the inline I/O groups page is showing inputs (true) or outputs (false)
    let inline_io_groups_is_input: Rc<Cell<bool>> = Rc::new(Cell::new(true));
    let chain_insert_window =
        ChainInsertWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    {
        use slint::Global;
        crate::Locale::get(&chain_insert_window).set_font_family(boot_font.into());
    }
    let insert_send_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    let insert_return_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    let block_editor_window =
        BlockEditorWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    {
        use slint::Global;
        crate::Locale::get(&block_editor_window).set_font_family(boot_font.into());
    }
    let tuner_window = TunerWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    {
        use slint::Global;
        crate::Locale::get(&tuner_window).set_font_family(boot_font.into());
    }
    let tuner_session: Rc<RefCell<Option<crate::tuner_session::TunerSession>>> =
        Rc::new(RefCell::new(None));
    let tuner_timer = Rc::new(Timer::default());
    let spectrum_window = SpectrumWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    {
        use slint::Global;
        crate::Locale::get(&spectrum_window).set_font_family(boot_font.into());
    }
    let spectrum_session: Rc<RefCell<Option<crate::spectrum_session::SpectrumSession>>> =
        Rc::new(RefCell::new(None));
    let spectrum_timer = Rc::new(Timer::default());

    // settings::language needs to know how to push the new font to every Window
    // (each Slint Window is a separate root with its own Locale global, so a
    // single set on AppWindow doesn't reach the secondary windows).
    {
        use slint::Global;
        let weak_app = window.as_weak();
        let weak_proj = project_settings_window.as_weak();
        let weak_chain_in = chain_input_window.as_weak();
        let weak_chain_out = chain_output_window.as_weak();
        let weak_chain_in_groups = chain_input_groups_window.as_weak();
        let weak_chain_out_groups = chain_output_groups_window.as_weak();
        let weak_chain_insert = chain_insert_window.as_weak();
        let weak_block_editor = block_editor_window.as_weak();
        let weak_tuner = tuner_window.as_weak();
        let weak_spectrum = spectrum_window.as_weak();
        let chain_editor_window_for_apply = chain_editor_window.clone();
        let plugin_info_window_for_apply = plugin_info_window.clone();
        let apply_font_to_all = move |font: &str| {
            let f = || -> slint::SharedString { font.into() };
            if let Some(w) = weak_app.upgrade() {
                crate::Locale::get(&w).set_font_family(f());
            }
            if let Some(w) = weak_proj.upgrade() {
                crate::Locale::get(&w).set_font_family(f());
            }
            if let Some(w) = weak_chain_in.upgrade() {
                crate::Locale::get(&w).set_font_family(f());
            }
            if let Some(w) = weak_chain_out.upgrade() {
                crate::Locale::get(&w).set_font_family(f());
            }
            if let Some(w) = weak_chain_in_groups.upgrade() {
                crate::Locale::get(&w).set_font_family(f());
            }
            if let Some(w) = weak_chain_out_groups.upgrade() {
                crate::Locale::get(&w).set_font_family(f());
            }
            if let Some(w) = weak_chain_insert.upgrade() {
                crate::Locale::get(&w).set_font_family(f());
            }
            if let Some(w) = weak_block_editor.upgrade() {
                crate::Locale::get(&w).set_font_family(f());
            }
            if let Some(w) = weak_tuner.upgrade() {
                crate::Locale::get(&w).set_font_family(f());
            }
            if let Some(w) = weak_spectrum.upgrade() {
                crate::Locale::get(&w).set_font_family(f());
            }
            if let Some(w) = chain_editor_window_for_apply.borrow().as_ref() {
                crate::Locale::get(w).set_font_family(f());
            }
            if let Some(w) = plugin_info_window_for_apply.borrow().as_ref() {
                crate::Locale::get(w).set_font_family(f());
            }
        };
        crate::settings::language::wire(
            &window,
            &project_settings_window,
            project_session.clone(),
            apply_font_to_all,
        );
    }
    // #712: System / Integrations master switches (MIDI adapter / MCP server).
    crate::settings::integrations::wire(
        &window,
        &project_settings_window,
        project_session.clone(),
        app_config.clone(),
    );
    // #716: System / I/O bindings editor.
    crate::settings::io_bindings::wire(
        &window,
        &project_settings_window,
        project_session.clone(),
        app_config.clone(),
        input_chain_devices.clone(),
        output_chain_devices.clone(),
    );
    let input_devices = Rc::new(VecModel::from(build_device_selection_items(
        &*input_chain_devices.borrow(),
        &settings.input_devices,
    )));
    mark_unselected_devices(&input_devices, &settings.input_devices);
    let output_devices = Rc::new(VecModel::from(build_device_selection_items(
        &*output_chain_devices.borrow(),
        &settings.output_devices,
    )));
    mark_unselected_devices(&output_devices, &settings.output_devices);

    // Initial AppWindow + ProjectSettingsWindow state + project-row VecModels
    // (extracted to desktop_app_init)
    let crate::desktop_app_init::InitialState {
        project_devices,
        project_chains,
        recent_projects,
        chain_input_device_options,
        chain_output_device_options,
        chain_input_channels,
        chain_output_channels,
    } = crate::desktop_app_init::populate_initial_window_state(
        &window,
        &project_settings_window,
        &context,
        &settings,
        auto_save,
        fullscreen,
        needs_audio_settings,
        &input_chain_devices,
        &output_chain_devices,
        &app_config,
        &input_devices,
        &output_devices,
    );

    // CLI auto-open (extracted to desktop_app_cli_open)
    crate::desktop_app_cli_open::try_auto_open(
        cli_project_path.as_ref(),
        &window,
        &project_session,
        &project_chains,
        &input_chain_devices,
        &output_chain_devices,
        &saved_project_snapshot,
        &project_dirty,
        &app_config,
        &recent_projects,
    );
    let crate::desktop_app_block_models::BlockEditorModels {
        block_type_options,
        block_model_options,
        filtered_block_model_options,
        block_model_option_labels,
        block_parameter_items,
        multi_slider_points,
        curve_editor_points,
        eq_band_curves,
    } = crate::desktop_app_block_models::init(&window, &block_editor_window);
    let block_editor_persist_timer = Rc::new(Timer::default());
    let toast_timer = Rc::new(Timer::default());
    window.set_toast_message("".into());
    window.set_toast_level("info".into());

    // Background polling timers (extracted to desktop_app_polling)
    crate::desktop_app_polling::start(
        &window,
        toast_timer.clone(),
        project_runtime.clone(),
        project_session.clone(),
    );

    // Issue #496 / #32 / #36: per-chain IN/OUT dBFS meter polling.
    // ~30 Hz timer that subscribes new chains' input + stream taps
    // and writes peak dBFS into the matching ProjectChainItem rows.
    crate::meter_wiring::start_meter_polling(
        project_runtime.clone(),
        project_chains.clone(),
        project_session.clone(),
    );

    // --- BlockEditorWindow callbacks (extracted to block_editor_window_wiring) ---
    crate::block_editor_window_wiring::wire(
        &window,
        &block_editor_window,
        crate::block_editor_window_wiring::BlockEditorWindowCtx {
            plugin_info_window: plugin_info_window.clone(),
        },
    );
    // --- ChainInput/ChainOutput picker callbacks (extracted to chain_io_picker_wiring) ---
    crate::chain_io_picker_wiring::wire(
        &window,
        &chain_input_window,
        &chain_output_window,
        crate::chain_io_picker_wiring::ChainIoPickerCtx {
            chain_draft: chain_draft.clone(),
        },
    );
    project_settings_window.set_project_devices(ModelRc::from(project_devices.clone()));
    window.set_project_devices(ModelRc::from(project_devices.clone()));
    project_settings_window.set_sample_rate_options(window.get_sample_rate_options());
    project_settings_window.set_buffer_size_options(window.get_buffer_size_options());
    project_settings_window.set_bit_depth_options(window.get_bit_depth_options());
    chain_input_window.set_device_options(ModelRc::from(chain_input_device_options.clone()));
    chain_input_window.set_channels(ModelRc::from(chain_input_channels.clone()));
    chain_input_window.set_selected_device_index(-1);
    chain_input_window.set_status_message("".into());
    chain_output_window.set_device_options(ModelRc::from(chain_output_device_options.clone()));
    chain_output_window.set_channels(ModelRc::from(chain_output_channels.clone()));
    chain_output_window.set_selected_device_index(-1);
    chain_output_window.set_status_message("".into());
    chain_insert_window.set_send_device_options(ModelRc::from(chain_output_device_options.clone()));
    chain_insert_window
        .set_return_device_options(ModelRc::from(chain_input_device_options.clone()));
    chain_insert_window.set_send_channels(ModelRc::from(insert_send_channels.clone()));
    chain_insert_window.set_return_channels(ModelRc::from(insert_return_channels.clone()));
    chain_insert_window.set_selected_send_device_index(-1);
    chain_insert_window.set_selected_return_device_index(-1);
    chain_insert_window.set_status_message("".into());
    // --- ChainInsertWindow callbacks (extracted to insert_wiring) ---
    crate::insert_wiring::wire(
        &window,
        &chain_insert_window,
        crate::insert_wiring::InsertWiringCtx {
            insert_draft: insert_draft.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            insert_send_channels: insert_send_channels.clone(),
            insert_return_channels: insert_return_channels.clone(),
            project_session: project_session.clone(),
            project_runtime: project_runtime.clone(),
            project_chains: project_chains.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            auto_save,
        },
    );
    // --- Device settings callbacks (extracted to device_settings_wiring) ---
    crate::device_settings_wiring::wire(
        &window,
        &project_settings_window,
        crate::device_settings_wiring::DeviceSettingsCtx {
            input_devices: input_devices.clone(),
            output_devices: output_devices.clone(),
            project_devices: project_devices.clone(),
        },
    );
    // Refresh devices — re-enumerates audio interfaces after a USB hot-swap.
    // Wired on both the standalone settings window and the inline (fullscreen)
    // settings page on the main window. Safe to call: the underlying
    // enumeration runs in the UI thread and is rate-limited by user clicks
    // (no periodic polling — that triggered scarlett2_notify freezes on
    // the Orange Pi USB-C OTG port).
    // --- Refresh devices callbacks (extracted to device_refresh_wiring) ---
    crate::device_refresh_wiring::wire(
        &window,
        &project_settings_window,
        crate::device_refresh_wiring::DeviceRefreshCtx {
            project_session: project_session.clone(),
            project_devices: project_devices.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            toast_timer: toast_timer.clone(),
            app_config: app_config.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
        },
    );
    // --- Audio wizard step nav callbacks (extracted to audio_wizard_wiring) ---
    crate::audio_wizard_wiring::wire(
        &window,
        crate::audio_wizard_wiring::AudioWizardCtx {
            input_devices: input_devices.clone(),
            toast_timer: toast_timer.clone(),
        },
    );
    // --- Audio settings save callbacks (extracted to settings::audio) ---
    crate::settings::audio::wire(
        &window,
        &project_settings_window,
        crate::settings::audio::AudioSettingsSaveCtx {
            input_devices: input_devices.clone(),
            output_devices: output_devices.clone(),
            project_devices: project_devices.clone(),
            audio_settings_mode: audio_settings_mode.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
            app_config: app_config.clone(),
        },
    );
    // --- System / MIDI devices section (#513) ---
    // Seed the in-memory row list from the persisted AppConfig and bind
    // it to the Slint model the section reads from. Each user edit
    // dispatches `SaveMidiDevices` (when a session is loaded) and
    // persists into config.yaml in the same callback — see
    // `crate::settings::midi_devices` for the rationale.
    let midi_device_rows: Rc<RefCell<Vec<infra_filesystem::MidiDeviceSelection>>> =
        Rc::new(RefCell::new(
            infra_filesystem::FilesystemStorage::load_app_config()
                .ok()
                .map(|c| c.midi_devices)
                .unwrap_or_default(),
        ));
    let midi_device_model: Rc<VecModel<crate::MidiDeviceRow>> = Rc::new(VecModel::default());
    crate::settings::midi_devices::replace_model(&midi_device_model, &midi_device_rows.borrow());
    crate::settings::midi_devices::install(
        &window,
        project_session.clone(),
        midi_device_rows.clone(),
        midi_device_model.clone(),
    );
    crate::settings::midi_devices::install_secondary(
        &project_settings_window,
        project_session.clone(),
        midi_device_rows.clone(),
        midi_device_model.clone(),
    );
    window.set_midi_devices(ModelRc::from(midi_device_model.clone()));
    project_settings_window.set_midi_devices(ModelRc::from(midi_device_model.clone()));
    // --- Project / Metadata section (#513) ---
    let last_dispatched_name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    crate::settings::project_meta::install(
        &window,
        project_session.clone(),
        last_dispatched_name.clone(),
    );
    crate::settings::project_meta::install_secondary(
        &project_settings_window,
        project_session.clone(),
        last_dispatched_name.clone(),
    );
    // --- System / Paths section (#513) ---
    crate::settings::paths::install(&window, project_session.clone(), app_config.clone());
    crate::settings::paths::install_secondary(
        &project_settings_window,
        project_session.clone(),
        app_config.clone(),
    );
    crate::settings::paths::seed_initial(&window);
    crate::settings::paths::seed_initial_secondary(&project_settings_window);
    // Seed the initial project-name and project-path-display from the session.
    {
        let sess = project_session.borrow();
        let name: slint::SharedString = sess
            .as_ref()
            .and_then(|s| s.project.borrow().name.clone())
            .unwrap_or_default()
            .into();
        let path: slint::SharedString = sess
            .as_ref()
            .and_then(|s| s.project_path.as_ref().map(|p| p.display().to_string()))
            .unwrap_or_else(|| "(unsaved)".into())
            .into();
        window.set_project_name(name.clone());
        window.set_project_path_display(path.clone());
        // Mirror onto the standalone settings window (#513): SettingsPage
        // reads project-name from project-name-draft, but the path is a
        // separate property that must be pushed independently.
        project_settings_window.set_project_name_draft(name);
        project_settings_window.set_project_path_display(path);
    }
    // --- Project file dialog callbacks (extracted to project_file_dialog_wiring) ---
    crate::project_file_dialog_wiring::wire(
        &window,
        crate::project_file_dialog_wiring::ProjectFileDialogCtx {
            project_paths: project_paths.clone(),
            app_config: app_config.clone(),
            recent_projects: recent_projects.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
        },
    );
    // --- Recent projects callbacks (extracted to recent_projects_wiring) ---
    crate::recent_projects_wiring::wire(
        &window,
        crate::recent_projects_wiring::RecentProjectsCtx {
            app_config: app_config.clone(),
            recent_projects: recent_projects.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
        },
    );
    // --- Project settings callbacks (extracted to project_settings_wiring) ---
    crate::project_settings_wiring::wire(
        &window,
        &project_settings_window,
        crate::project_settings_wiring::ProjectSettingsCtx {
            project_session: project_session.clone(),
            project_devices: project_devices.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            audio_settings_mode: audio_settings_mode.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
            fullscreen,
        },
    );
    // --- Chain preset callbacks (extracted to chain_preset_wiring) ---
    crate::chain_preset_wiring::wire(
        &window,
        crate::chain_preset_wiring::ChainPresetCtx {
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            preset_file_list: preset_file_list.clone(),
            auto_save,
        },
    );
    latency_probe::install_handler(
        &window,
        project_session.clone(),
        project_chains.clone(),
        probe_windows.clone(),
    );
    // ── Tuner window — top-bar feature ──
    crate::tuner_wiring::wire_tuner(
        &window,
        &tuner_window,
        &project_session,
        &project_runtime,
        &tuner_session,
        &tuner_timer,
    );
    // ── Spectrum window — top-bar feature ──
    crate::spectrum_wiring::wire_spectrum(
        &window,
        &spectrum_window,
        &project_session,
        &project_runtime,
        &spectrum_session,
        &spectrum_timer,
    );
    // --- Back-to-launcher callback (extracted to back_to_launcher_wiring) ---
    crate::back_to_launcher_wiring::wire(
        &window,
        &project_settings_window,
        &block_editor_window,
        crate::back_to_launcher_wiring::BackToLauncherCtx {
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            chain_editor_window: chain_editor_window.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
        },
    );
    // --- Chain-level callback wirings (extracted to desktop_app_chain_wiring) ---
    crate::desktop_app_chain_wiring::wire_all(&crate::desktop_app_chain_wiring::ChainWiringDeps {
        window: &window,
        chain_input_window: &chain_input_window,
        chain_output_window: &chain_output_window,
        chain_input_groups_window: &chain_input_groups_window,
        chain_output_groups_window: &chain_output_groups_window,
        chain_draft: chain_draft.clone(),
        block_editor_draft: block_editor_draft.clone(),
        io_block_insert_draft: io_block_insert_draft.clone(),
        inline_io_groups_is_input: inline_io_groups_is_input.clone(),
        project_session: project_session.clone(),
        project_chains: project_chains.clone(),
        project_runtime: project_runtime.clone(),
        saved_project_snapshot: saved_project_snapshot.clone(),
        project_dirty: project_dirty.clone(),
        input_chain_devices: input_chain_devices.clone(),
        output_chain_devices: output_chain_devices.clone(),
        chain_input_device_options: chain_input_device_options.clone(),
        chain_output_device_options: chain_output_device_options.clone(),
        chain_input_channels: chain_input_channels.clone(),
        chain_output_channels: chain_output_channels.clone(),
        chain_editor_window: chain_editor_window.clone(),
        open_compact_window: open_compact_window.clone(),
        vst3_editor_handles: vst3_editor_handles.clone(),
        toast_timer: toast_timer.clone(),
        app_config: app_config.clone(),
        vst3_sample_rate,
        fullscreen,
        auto_save,
    });
    // --- Block-related callback wirings (extracted to desktop_app_block_wiring) ---
    crate::desktop_app_block_wiring::wire_all(&crate::desktop_app_block_wiring::BlockWiringDeps {
        window: &window,
        block_editor_window: &block_editor_window,
        chain_input_window: &chain_input_window,
        chain_output_window: &chain_output_window,
        chain_input_groups_window: &chain_input_groups_window,
        chain_output_groups_window: &chain_output_groups_window,
        chain_insert_window: &chain_insert_window,
        selected_block: selected_block.clone(),
        block_editor_draft: block_editor_draft.clone(),
        chain_draft: chain_draft.clone(),
        insert_draft: insert_draft.clone(),
        io_block_insert_draft: io_block_insert_draft.clone(),
        block_type_options: block_type_options.clone(),
        block_model_options: block_model_options.clone(),
        filtered_block_model_options: filtered_block_model_options.clone(),
        block_model_option_labels: block_model_option_labels.clone(),
        block_parameter_items: block_parameter_items.clone(),
        multi_slider_points: multi_slider_points.clone(),
        curve_editor_points: curve_editor_points.clone(),
        eq_band_curves: eq_band_curves.clone(),
        project_session: project_session.clone(),
        project_chains: project_chains.clone(),
        project_runtime: project_runtime.clone(),
        saved_project_snapshot: saved_project_snapshot.clone(),
        project_dirty: project_dirty.clone(),
        input_chain_devices: input_chain_devices.clone(),
        output_chain_devices: output_chain_devices.clone(),
        chain_input_device_options: chain_input_device_options.clone(),
        chain_output_device_options: chain_output_device_options.clone(),
        chain_input_channels: chain_input_channels.clone(),
        chain_output_channels: chain_output_channels.clone(),
        insert_send_channels: insert_send_channels.clone(),
        insert_return_channels: insert_return_channels.clone(),
        open_block_windows: open_block_windows.clone(),
        inline_stream_timer: inline_stream_timer.clone(),
        open_compact_window: open_compact_window.clone(),
        toast_timer: toast_timer.clone(),
        plugin_info_window: plugin_info_window.clone(),
        vst3_editor_handles: vst3_editor_handles.clone(),
        vst3_editor_handles_for_on_open: vst3_editor_handles_for_on_open.clone(),
        block_editor_persist_timer: block_editor_persist_timer.clone(),
        vst3_sample_rate,
        auto_save,
    });
    // Fullscreen inline chain editor callbacks — delegate to ChainEditorWindow
    // --- Chain editor delegation forwarders (extracted to chain_editor_forwarders_wiring) ---
    crate::chain_editor_forwarders_wiring::wire(&window, chain_editor_window.clone());
    // --- Fullscreen I/O editor + groups callbacks (extracted to chain_io_fullscreen_callbacks) ---
    crate::chain_io_fullscreen_callbacks::wire(
        &window,
        &chain_input_window,
        &chain_output_window,
        &chain_input_groups_window,
        &chain_output_groups_window,
        crate::chain_io_fullscreen_callbacks::ChainIoFullscreenCallbacksCtx {
            chain_editor_window: chain_editor_window.clone(),
            inline_io_groups_is_input: inline_io_groups_is_input.clone(),
            chain_draft: chain_draft.clone(),
            project_session: project_session.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            chain_input_channels: chain_input_channels.clone(),
            chain_output_channels: chain_output_channels.clone(),
            app_config: app_config.clone(),
        },
    );
    // --- on_save_chain + on_cancel_chain (extracted to chain_save_cancel_callbacks) ---
    crate::chain_save_cancel_callbacks::wire(
        &window,
        crate::chain_save_cancel_callbacks::ChainSaveCancelCtx {
            chain_draft: chain_draft.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
        },
    );
    // --- Chain I/O save/cancel callbacks (extracted to chain_io_save_wiring) ---
    crate::chain_io_save_wiring::wire(
        &window,
        &chain_input_window,
        &chain_output_window,
        &chain_input_groups_window,
        &chain_output_groups_window,
        crate::chain_io_save_wiring::ChainIoSaveCtx {
            chain_draft: chain_draft.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            chain_editor_window: chain_editor_window.clone(),
            io_block_insert_draft: io_block_insert_draft.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
        },
    );
    // --- Chain row callbacks (extracted to chain_row_wiring) ---
    crate::chain_row_wiring::wire(
        &window,
        crate::chain_row_wiring::ChainRowCtx {
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
            pending_delete_chain_id: std::rc::Rc::new(std::cell::RefCell::new(None)),
        },
    );
    // #614: DI loop file picker — separate module because chain_row_wiring
    // is forbidden from using rfd:: (issue #511).
    crate::di_loop_chooser_wiring::wire(
        &window,
        project_session.clone(),
        toast_timer.clone(),
    );
    crate::chain_rig_nav_wiring::wire(
        &window,
        crate::chain_rig_nav_wiring::ChainRigNavCtx {
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            auto_save,
        },
    );
    crate::plugin_info_inline_wiring::wire(&window);
    // Ao fechar a janela principal, encerra todo o processo
    window.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });

    // Per-chain latency badge: measurement is taken synchronously by
    // `latency_probe::install_handler`; this timer only clears each
    // badge after its 10-second display window expires.
    let _latency_timer = latency_probe::install_expiry_timer(
        window.as_weak(),
        project_chains.clone(),
        probe_windows.clone(),
    );

    // Virtual keyboard: dispatch key events to the focused element
    // Virtual keyboard (extracted to virtual_keyboard_wiring)
    crate::virtual_keyboard_wiring::wire(&window);

    // ── MCP server (opt-in, --mcp[=addr]) ──────────────────────────────────
    // A complementary network server on the live instance: an agent drives
    // the same `ProjectSession` the user has open. The server runs on its own
    // thread (tokio); commands/queries cross the `!Send` boundary via the
    // bridge and are serviced here on the Slint event-loop thread (same place
    // GUI callbacks dispatch), so GUI and MCP share one project with no lock.
    // Bound for the whole `window.run()` so the timer keeps firing.
    let _mcp_drain_timer = if let Some(addr) = mcp_addr {
        let (bridge, drain) = application::bridge::channel();
        std::thread::Builder::new()
            .name("openrig-mcp".into())
            .spawn(move || {
                if let Err(e) = adapter_mcp::run_blocking(bridge, addr) {
                    log::error!("MCP server stopped: {e}");
                }
            })?;
        log::info!("MCP server listening on http://{addr}");
        let session_for_mcp = project_session.clone();
        let mcp_ctx = crate::chain_rig_nav_wiring::ChainRigNavCtx {
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            auto_save,
        };
        let mcp_window = window.as_weak();
        // Cloned for the meter resolver closure (the moves above
        // consumed `project_chains` for the rig-nav ctx).
        let chains_for_meters = mcp_ctx.project_chains.clone();
        let timer = Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(16),
            move || {
                // Drain + serve queries under the session borrow, then drop
                // it before refreshing (apply_events_to_ui re-borrows it).
                let events = {
                    let session_borrow = session_for_mcp.borrow();
                    let Some(session) = session_borrow.as_ref() else {
                        return;
                    };
                    let mut events = drain.drain(session.dispatcher.as_ref(), 32);
                    // #693: completions of off-thread command work (DI
                    // decode, ...) ride the same event path as a dispatch.
                    {
                        use application::dispatcher::CommandDispatcher as _;
                        events.extend(session.dispatcher.poll_async_results());
                    }
                    let project = &session.project;
                    drain.serve_queries(
                        |kind| match kind {
                            application::bridge::QueryKind::ProjectYaml => {
                                infra_yaml::serialize_project(&project.borrow())
                                    .map_err(|e| e.to_string())
                            }
                            application::bridge::QueryKind::Devices => infra_cpal::list_devices()
                                .map(|d| d.join("\n"))
                                .map_err(|e| e.to_string()),
                            application::bridge::QueryKind::Ids => {
                                Ok(application::query::list_ids(&project.borrow()))
                            }
                            application::bridge::QueryKind::ChainMeters => {
                                use slint::Model;
                                let proj_borrow = project.borrow();
                                let mut out = String::new();
                                for (idx, chain) in proj_borrow.chains.iter().enumerate() {
                                    let row = chains_for_meters.row_data(idx);
                                    let (in_db, out_db) = row
                                        .map(|r| (r.meter_in_dbfs, r.meter_out_dbfs))
                                        .unwrap_or((
                                            engine::output_meter::SILENT_DBFS,
                                            engine::output_meter::SILENT_DBFS,
                                        ));
                                    out.push_str(&format!(
                                        "{}\t{:.1}\t{:.1}\n",
                                        chain.id.0, in_db, out_db
                                    ));
                                }
                                Ok(out)
                            }
                            application::bridge::QueryKind::ListChainPresets { chain } => {
                                // #554: the chain's preset bank, served
                                // from the in-memory RigProject so MCP /
                                // gRPC see the same list the GUI shows
                                // in the chain-title combobox.
                                match session.rig.as_ref() {
                                    Some(rig) => {
                                        application::query::list_chain_presets(&rig.borrow(), chain)
                                    }
                                    None => Err("no rig attached to the session".to_string()),
                                }
                            }
                            application::bridge::QueryKind::ListProjectPresets => {
                                // #554 follow-up: project-level preset
                                // pool (RigProject.presets in memory).
                                // A preset can sit here without being
                                // wired to any input bank yet.
                                match session.rig.as_ref() {
                                    Some(rig) => {
                                        Ok(application::query::list_project_presets(&rig.borrow()))
                                    }
                                    None => Err("no rig attached to the session".to_string()),
                                }
                            }
                            // #561 (expanded scope): plugin catalog
                            // reads — same pure helpers MCP would call
                            // (process-wide registry, no project state).
                            application::bridge::QueryKind::ListPluginCatalog => {
                                Ok(application::query::list_plugin_catalog())
                            }
                            application::bridge::QueryKind::GetPlugin { id } => {
                                Ok(application::query::get_plugin(id))
                            }
                            application::bridge::QueryKind::FindPlugins { query } => {
                                Ok(application::query::find_plugins(query))
                            }
                            // #572: per-plugin parameter schema
                            // (catalog-level). No project state needed —
                            // resolves against the process-wide plugin
                            // registry, same as the catalog reads above.
                            application::bridge::QueryKind::GetPluginParams { plugin_id } => {
                                Ok(application::query::get_plugin_params(plugin_id))
                            }
                            // #572: per-block-instance descriptors
                            // (schema + current value). Reads from the
                            // live project the GUI session owns.
                            application::bridge::QueryKind::GetBlockParams { chain, block } => {
                                application::query::get_block_params(
                                    &project.borrow(),
                                    chain,
                                    block,
                                )
                            }
                            // #582: effective resolved system paths
                            // (data root + every configurable directory)
                            // for the `openrig://paths` MCP resource.
                            // Loads from `config.yaml` on each call so
                            // path overrides written via
                            // `Command::Set*Path` are visible immediately
                            // without restarting the process.
                            application::bridge::QueryKind::Paths => {
                                Ok(application::query::resolved_paths_json())
                            }
                        },
                        32,
                    );
                    events
                };
                if events.is_empty() {
                    return;
                }
                if let Some(window) = mcp_window.upgrade() {
                    crate::chain_rig_nav_wiring::apply_events_to_ui(&window, &mcp_ctx, &events);
                }
            },
        );
        Some(timer)
    } else {
        None
    };

    // ── MIDI/BLE-MIDI controller adapter (opt-in, --midi[=PATH]) ───────────
    // Same complementary-input pattern as MCP; wiring extracted to keep this
    // file within the size cap. Bound for the whole `window.run()`.
    let _midi_drain_timer = match midi_map {
        Some(arg) => Some(crate::midi_adapter_wiring::wire(
            window.as_weak(),
            crate::chain_rig_nav_wiring::ChainRigNavCtx {
                project_session: project_session.clone(),
                project_chains: project_chains.clone(),
                project_runtime: project_runtime.clone(),
                input_chain_devices: input_chain_devices.clone(),
                output_chain_devices: output_chain_devices.clone(),
                toast_timer: toast_timer.clone(),
                saved_project_snapshot: saved_project_snapshot.clone(),
                project_dirty: project_dirty.clone(),
                auto_save,
            },
            arg,
        )?),
        None => None,
    };

    window.run().map_err(|error| anyhow!(error.to_string()))
}
