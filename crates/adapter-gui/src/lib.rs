mod thumbnails;
mod plugin_info;
mod spectrum_session;
mod spectrum_wiring;
mod tuner_session;
mod tuner_wiring;
mod insert_wiring;
mod device_settings_wiring;
mod chain_io_picker_wiring;
mod block_editor_window_wiring;
mod recent_projects_wiring;
mod project_file_dialog_wiring;
mod device_refresh_wiring;
mod audio_wizard_wiring;
mod project_settings_wiring;
mod back_to_launcher_wiring;
mod chain_preset_wiring;
mod audio_settings_save_wiring;
mod chain_crud_wiring;
mod chain_io_main_wiring;
mod chain_input_groups_wiring;
mod chain_output_groups_wiring;
mod chain_row_wiring;
mod chain_editor_forwarders_wiring;
mod chain_block_crud_wiring;
mod virtual_keyboard_wiring;
mod chain_name_wiring;
mod chain_editor_callbacks;
mod chain_io_save_wiring;
mod block_model_search_wiring;
mod block_picker_wiring;
mod block_drawer_close_wiring;
mod block_delete_wiring;
mod vst3_editor_wiring;
mod block_drawer_save_delete_wiring;
mod block_parameter_wiring;
mod compact_chain_callbacks;
mod compact_chain_block_handlers;
mod compact_chain_param_handlers;
mod block_insert_callbacks;
mod block_choose_type_callback;
mod chain_io_fullscreen_callbacks;
mod runtime_lifecycle;
mod select_chain_block_callback;
mod block_editor_window_setup;
mod block_editor_window_params;
mod block_editor_window_lifecycle;
mod cli;
mod chain_save_cancel_callbacks;
pub use cli::parse_cli_args_from;
pub(crate) use chain_editor_callbacks::setup_chain_editor_callbacks;
pub(crate) use runtime_lifecycle::{
    assign_new_block_ids, remove_live_chain_runtime, stop_project_runtime,
    sync_live_chain_runtime, sync_project_runtime, system_language,
    ui_index_to_real_block_index,
};

use anyhow::{anyhow, Result};

const SELECT_PATH_PREFIX: &str = "__select.";
const SELECT_SELECTED_BLOCK_ID: &str = "__select.selected_block_id";
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use infra_filesystem::FilesystemStorage;
use slint::{ModelRc, SharedString, Timer, VecModel};

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use ui_openrig::{AppRuntimeMode, InteractionMode, UiRuntimeContext};
mod audio_devices;
mod block_editor;
mod chain_editor;
mod eq;
mod helpers;
mod io_groups;
mod latency_probe;
mod project_ops;
mod project_view;
mod state;
mod model_search;
mod model_search_wiring;
mod ui_state;
mod visual_config;
slint::include_modules!();
use state::{
    ProjectSession,
    ChainDraft, SelectedBlock, BlockEditorDraft, IoBlockInsertDraft, InsertDraft,
    AudioSettingsMode, UNTITLED_PROJECT_NAME, BlockWindow,
};
use audio_devices::{
    build_project_device_rows,
    default_device_settings, normalize_device_settings, mark_unselected_devices,
};
use helpers::{
    set_status_error, set_status_info, set_status_warning,
};
use project_ops::{
    open_cli_project, resolve_project_paths, load_and_sync_app_config,
    canonical_project_path, register_recent_project,
    recent_project_items, project_display_name,
    project_session_snapshot,
    set_project_dirty,
    project_title_for_path,
};
use project_view::{
    block_type_picker_items,
    replace_project_chains,
};
const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const DEFAULT_BUFFER_SIZE_FRAMES: u32 = 64;
const DEFAULT_BIT_DEPTH: u32 = 32;
const SUPPORTED_SAMPLE_RATES: &[u32] = &[44_100, 48_000, 88_200, 96_000];
const SUPPORTED_BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];
const SUPPORTED_BIT_DEPTHS: &[u32] = &[16, 24, 32];

pub fn run_desktop_app(
    runtime_mode: AppRuntimeMode,
    interaction_mode: InteractionMode,
    cli_project_path: Option<PathBuf>,
    auto_save: bool,
    fullscreen: bool,
) -> Result<()> {
    log::info!("starting desktop app: runtime_mode={:?}, interaction_mode={:?}", runtime_mode, interaction_mode);
    let context = UiRuntimeContext::new(runtime_mode, interaction_mode);
    let settings = FilesystemStorage::load_gui_audio_settings()?.unwrap_or_default();
    let needs_audio_settings =
        context.capabilities.can_select_audio_device && !settings.is_complete();
    let project_paths = resolve_project_paths();
    let loaded_config = load_and_sync_app_config()?;
    let resolved_paths = infra_filesystem::resolve_asset_paths(loaded_config.paths.clone());
    infra_filesystem::init_asset_paths(resolved_paths);
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
    let open_compact_window: Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>> = Rc::new(RefCell::new(None));
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
    window.window().set_size(slint::WindowSize::Logical(slint::LogicalSize {
        width: 1100.0,
        height: 620.0,
    }));
    let project_settings_window =
        ProjectSettingsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>> =
        Rc::new(RefCell::new(None));
    let plugin_info_window: Rc<RefCell<Option<PluginInfoWindow>>> = Rc::new(RefCell::new(None));
    let chain_input_window =
        ChainInputWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let chain_output_window =
        ChainOutputWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let chain_input_groups_window =
        ChainInputGroupsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let chain_output_groups_window =
        ChainOutputGroupsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    // Tracks whether the inline I/O groups page is showing inputs (true) or outputs (false)
    let inline_io_groups_is_input: Rc<Cell<bool>> = Rc::new(Cell::new(true));
    let chain_insert_window =
        ChainInsertWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let insert_send_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    let insert_return_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    let block_editor_window =
        BlockEditorWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let tuner_window =
        TunerWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let tuner_session: Rc<RefCell<Option<tuner_session::TunerSession>>> =
        Rc::new(RefCell::new(None));
    let tuner_timer = Rc::new(Timer::default());
    let spectrum_window =
        SpectrumWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let spectrum_session: Rc<RefCell<Option<spectrum_session::SpectrumSession>>> =
        Rc::new(RefCell::new(None));
    let spectrum_timer = Rc::new(Timer::default());
    window.set_app_version(env!("CARGO_PKG_VERSION").into());
    window.set_show_project_launcher(true);
    window.set_show_project_setup(false);
    window.set_show_project_chains(false);
    window.set_show_chain_editor(false);
    window.set_show_project_settings(false);
    window.set_project_dirty(false);
    window.set_project_path_label("".into());
    window.set_project_title("Projeto".into());
    window.set_project_name_draft("".into());
    window.set_recent_project_search("".into());
    window.set_chain_editor_title("Nova chain".into());
    window.set_chain_editor_save_label("Criar chain".into());
    window.set_runtime_mode_label(context.runtime_mode.label().into());
    window.set_interaction_mode_label(context.interaction_mode.label().into());
    window.set_touch_optimized(context.capabilities.touch_optimized);
    window.set_auto_save(auto_save);
    window.set_fullscreen(fullscreen);
    if fullscreen {
        window.window().set_fullscreen(true);
    }
    window.set_show_audio_settings(needs_audio_settings);
    window.set_wizard_step(if settings.is_complete() { 1 } else { 0 });
    window.set_status_message("".into());
    let input_devices = Rc::new(VecModel::from(
        input_chain_devices
            .borrow()
            .iter()
            .map(|device| {
                let device_id = device.id.clone();
                let name = device.name.clone();
                let config = settings
                    .input_devices
                    .iter()
                    .find(|saved| saved.device_id == device_id)
                    .cloned()
                    .map(normalize_device_settings)
                    .unwrap_or_else(|| default_device_settings(device_id.clone(), name.clone()));
                DeviceSelectionItem {
                    device_id: config.device_id.into(),
                    name: config.name.into(),
                    selected: true,
                    sample_rate_text: config.sample_rate.to_string().into(),
                    buffer_size_text: config.buffer_size_frames.to_string().into(),
                    bit_depth_text: config.bit_depth.to_string().into(),
                }
            })
            .collect::<Vec<_>>(),
    ));
    mark_unselected_devices(&input_devices, &settings.input_devices);
    let output_devices = Rc::new(VecModel::from(
        output_chain_devices
            .borrow()
            .iter()
            .map(|device| {
                let device_id = device.id.clone();
                let name = device.name.clone();
                let config = settings
                    .output_devices
                    .iter()
                    .find(|saved| saved.device_id == device_id)
                    .cloned()
                    .map(normalize_device_settings)
                    .unwrap_or_else(|| default_device_settings(device_id.clone(), name.clone()));
                DeviceSelectionItem {
                    device_id: config.device_id.into(),
                    name: config.name.into(),
                    selected: true,
                    sample_rate_text: config.sample_rate.to_string().into(),
                    buffer_size_text: config.buffer_size_frames.to_string().into(),
                    bit_depth_text: config.bit_depth.to_string().into(),
                }
            })
            .collect::<Vec<_>>(),
    ));
    mark_unselected_devices(&output_devices, &settings.output_devices);
    let project_devices = Rc::new(VecModel::from(build_project_device_rows(
        &*input_chain_devices.borrow(),
        &*output_chain_devices.borrow(),
        &[],
    )));
    window.set_input_devices(ModelRc::from(input_devices.clone()));
    window.set_output_devices(ModelRc::from(output_devices.clone()));
    let project_chains = Rc::new(VecModel::from(Vec::<ProjectChainItem>::new()));
    window.set_project_chains(ModelRc::from(project_chains.clone()));
    let recent_projects = Rc::new(VecModel::from(recent_project_items(
        &app_config.borrow().recent_projects,
        "",
    )));
    window.set_recent_projects(ModelRc::from(recent_projects.clone()));
    // CLI auto-open
    if let Some(ref cli_path) = cli_project_path {
        match open_cli_project(cli_path) {
            Ok(session) => {
                let canonical_path = canonical_project_path(cli_path).unwrap_or(cli_path.clone());
                let title = project_title_for_path(Some(&canonical_path), &session.project);
                let display_name = project_display_name(&session.project);
                replace_project_chains(
                    &project_chains,
                    &session.project,
                    &*input_chain_devices.borrow(),
                    &*output_chain_devices.borrow(),
                );
                let snapshot = project_session_snapshot(&session).ok();
                *project_session.borrow_mut() = Some(session);
                *saved_project_snapshot.borrow_mut() = snapshot;
                register_recent_project(
                    &mut app_config.borrow_mut(),
                    &canonical_path,
                    &display_name,
                );
                let _ = FilesystemStorage::save_app_config(&app_config.borrow());
                recent_projects.set_vec(recent_project_items(&app_config.borrow().recent_projects, ""));
                set_project_dirty(&window, &project_dirty, false);
                window.set_project_title(title.into());
                window.set_project_path_label(
                    format!("Projeto: {}", canonical_path.display()).into(),
                );
                window.set_show_project_launcher(false);
                window.set_show_project_setup(false);
                window.set_show_project_chains(true);
                window.set_skip_launcher(true);
                log::info!("CLI: opened {:?}", canonical_path);
            }
            Err(e) => {
                log::error!("CLI project open failed, falling back to launcher: {e}");
            }
        }
    }
    let chain_input_device_options = Rc::new(VecModel::from(
        input_chain_devices
            .borrow()
            .iter()
            .map(|device| SharedString::from(device.name.clone()))
            .collect::<Vec<_>>(),
    ));
    let chain_output_device_options = Rc::new(VecModel::from(
        output_chain_devices
            .borrow()
            .iter()
            .map(|device| SharedString::from(device.name.clone()))
            .collect::<Vec<_>>(),
    ));
    let chain_input_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    let chain_output_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    window.set_chain_input_device_options(ModelRc::from(chain_input_device_options.clone()));
    window.set_chain_output_device_options(ModelRc::from(chain_output_device_options.clone()));
    window.set_chain_input_channels(ModelRc::from(chain_input_channels.clone()));
    window.set_chain_output_channels(ModelRc::from(chain_output_channels.clone()));
    window.set_selected_chain_input_device_index(-1);
    window.set_selected_chain_output_device_index(-1);
    window.set_selected_chain_block_chain_index(-1);
    window.set_selected_chain_block_index(-1);
    window.set_show_block_type_picker(false);
    window.set_show_block_model_picker(false);
    window.set_block_picker_title("".into());
    window.set_show_block_drawer(false);
    window.set_block_drawer_title("".into());
    window.set_block_drawer_confirm_label("Adicionar".into());
    window.set_block_drawer_status_message("".into());
    window.set_block_drawer_edit_mode(false);
    window.set_block_drawer_selected_type_index(-1);
    window.set_block_drawer_selected_model_index(-1);
    window.set_block_drawer_enabled(true);
    window.set_chain_draft_name("".into());
    project_settings_window.set_status_message("".into());
    project_settings_window.set_project_name_draft("".into());
    let block_type_options = Rc::new(VecModel::from(block_type_picker_items(block_core::INST_GENERIC)));
    let block_model_options = Rc::new(VecModel::from(Vec::<BlockModelPickerItem>::new()));
    let filtered_block_model_options =
        Rc::new(VecModel::from(Vec::<BlockModelPickerItem>::new()));
    let block_model_option_labels = Rc::new(VecModel::from(Vec::<SharedString>::new()));
    let block_parameter_items = Rc::new(VecModel::from(Vec::<BlockParameterItem>::new()));
    let multi_slider_points = Rc::new(VecModel::from(Vec::<MultiSliderPoint>::new()));
    let curve_editor_points = Rc::new(VecModel::from(Vec::<CurveEditorPoint>::new()));
    let eq_band_curves = Rc::new(VecModel::from(Vec::<SharedString>::new()));
    let block_editor_persist_timer = Rc::new(Timer::default());
    let toast_timer = Rc::new(Timer::default());
    window.set_toast_message("".into());
    window.set_toast_level("info".into());

    // Error polling timer — drains block errors from the audio engine and shows toasts
    {
        let weak_window = window.as_weak();
        let toast_timer_for_errors = toast_timer.clone();
        let project_runtime_for_errors = project_runtime.clone();
        let error_poll_timer = Timer::default();
        error_poll_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(200),
            move || {
                let Some(win) = weak_window.upgrade() else { return; };
                let rt_borrow = project_runtime_for_errors.borrow();
                let Some(rt) = rt_borrow.as_ref() else { return; };
                let errors = rt.poll_errors();
                if let Some(first) = errors.first() {
                    set_status_error(&win, &toast_timer_for_errors, &format!("Plugin error: {}", first.message));
                }
            },
        );
        std::mem::forget(error_poll_timer);
    }

    // Audio health check timer — detects device disconnects (JACK server
    // down on Linux, CoreAudio device removed on macOS) and auto-reconnects
    // when the backend becomes available again.
    {
        let weak_window = window.as_weak();
        let toast_timer_health = toast_timer.clone();
        let runtime_health = project_runtime.clone();
        let session_health = project_session.clone();
        let disconnected = Rc::new(RefCell::new(false));
        let _input_opts_health = chain_input_device_options.clone();
        let _output_opts_health = chain_output_device_options.clone();
        let health_timer = Timer::default();
        health_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(2),
            move || {
                let Some(win) = weak_window.upgrade() else { return; };

                // NOTE: device hot-plug detection moved OUT of the health timer.
                // Periodically polling /proc/asound/cards while the Scarlett 4th Gen
                // is on the USB-C OTG port triggers scarlett2_notify 0x20000000 and
                // freezes the device. The device list now refreshes only when the
                // user enters a UI surface that needs it (chain I/O editor, Settings,
                // configure-project) — see the refresh_input_devices call sites.
                let mut rt_borrow = runtime_health.borrow_mut();
                let Some(rt) = rt_borrow.as_mut() else { return; };
                if !rt.is_running() {
                    return;
                }
                let mut is_disconnected = disconnected.borrow_mut();

                if rt.is_healthy() {
                    if *is_disconnected {
                        // Was disconnected, now healthy again — nothing to do,
                        // reconnection already happened
                        *is_disconnected = false;
                    }
                    return;
                }

                // Backend is unhealthy
                if !*is_disconnected {
                    *is_disconnected = true;
                    set_status_warning(&win, &toast_timer_health, "Audio device disconnected — reconnecting...");
                    log::warn!("health check: audio backend unhealthy, will attempt reconnection");
                }

                // Try to reconnect
                let session_borrow = session_health.borrow();
                let Some(session) = session_borrow.as_ref() else { return; };
                match rt.try_reconnect(&session.project) {
                    Ok(true) => {
                        *is_disconnected = false;
                        set_status_info(&win, &toast_timer_health, "Audio device reconnected");
                        log::info!("health check: successfully reconnected");
                    }
                    Ok(false) => {
                        log::debug!("health check: backend not ready yet, will retry");
                    }
                    Err(e) => {
                        log::warn!("health check: reconnection attempt failed: {}", e);
                    }
                }
            },
        );
        std::mem::forget(health_timer);
    }

    window.set_block_type_options(ModelRc::from(block_type_options.clone()));
    window.set_block_model_options(ModelRc::from(block_model_options.clone()));
    window.set_filtered_block_model_options(ModelRc::from(filtered_block_model_options.clone()));
    window.set_block_model_option_labels(ModelRc::from(block_model_option_labels.clone()));
    window.set_block_parameter_items(ModelRc::from(block_parameter_items.clone()));
    window.set_multi_slider_points(ModelRc::from(multi_slider_points.clone()));
    window.set_curve_editor_points(ModelRc::from(curve_editor_points.clone()));
    window.set_eq_band_curves(ModelRc::from(eq_band_curves.clone()));
    block_editor_window.set_block_type_options(ModelRc::from(block_type_options.clone()));
    block_editor_window.set_block_model_options(ModelRc::from(block_model_options.clone()));
    block_editor_window
        .set_filtered_block_model_options(ModelRc::from(filtered_block_model_options.clone()));
    block_editor_window
        .set_block_model_option_labels(ModelRc::from(block_model_option_labels.clone()));
    block_editor_window.set_block_parameter_items(ModelRc::from(block_parameter_items.clone()));
    block_editor_window.set_multi_slider_points(ModelRc::from(multi_slider_points.clone()));
    block_editor_window.set_curve_editor_points(ModelRc::from(curve_editor_points.clone()));
    block_editor_window.set_eq_band_curves(ModelRc::from(eq_band_curves.clone()));
    block_editor_window.set_block_drawer_title("".into());
    block_editor_window.set_block_drawer_confirm_label("Adicionar".into());
    block_editor_window.set_block_drawer_status_message("".into());
    block_editor_window.set_block_drawer_edit_mode(false);
    block_editor_window.set_block_drawer_selected_type_index(-1);
    block_editor_window.set_block_drawer_selected_model_index(-1);
    block_editor_window.set_block_drawer_enabled(true);
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
    chain_insert_window.set_return_device_options(ModelRc::from(chain_input_device_options.clone()));
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
    // --- Audio settings save callbacks (extracted to audio_settings_save_wiring) ---
    crate::audio_settings_save_wiring::wire(
        &window,
        &project_settings_window,
        crate::audio_settings_save_wiring::AudioSettingsSaveCtx {
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
        },
    );
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
    tuner_wiring::wire_tuner(
        &window,
        &tuner_window,
        &project_session,
        &project_runtime,
        &tuner_session,
        &tuner_timer,
    );
    // ── Spectrum window — top-bar feature ──
    spectrum_wiring::wire_spectrum(
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
    // --- Chain CRUD callbacks (extracted to chain_crud_wiring) ---
    crate::chain_crud_wiring::wire(
        &window,
        &chain_input_window,
        &chain_output_window,
        crate::chain_crud_wiring::ChainCrudCtx {
            project_session: project_session.clone(),
            chain_draft: chain_draft.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            chain_input_channels: chain_input_channels.clone(),
            chain_output_channels: chain_output_channels.clone(),
            chain_editor_window: chain_editor_window.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            io_block_insert_draft: io_block_insert_draft.clone(),
            toast_timer: toast_timer.clone(),
            auto_save,
            fullscreen,
        },
    );
    // --- on_open_compact_chain_view (extracted to compact_chain_callbacks) ---
    compact_chain_callbacks::wire(
        &window,
        compact_chain_callbacks::CompactChainCallbacksCtx {
            project_session: project_session.clone(),
            project_runtime: project_runtime.clone(),
            project_chains: project_chains.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            toast_timer: toast_timer.clone(),
            open_compact_window: open_compact_window.clone(),
            vst3_editor_handles: vst3_editor_handles.clone(),
            block_editor_draft: block_editor_draft.clone(),
            fullscreen,
            auto_save,
            vst3_sample_rate,
        },
    );
    // --- Chain name edit callback (extracted to chain_name_wiring) ---
    crate::chain_name_wiring::wire(&window, chain_draft.clone());
    // --- Chain I/O main-window callbacks (extracted to chain_io_main_wiring) ---
    crate::chain_io_main_wiring::wire(
        &window,
        &chain_input_window,
        &chain_output_window,
        &chain_input_groups_window,
        &chain_output_groups_window,
        crate::chain_io_main_wiring::ChainIoMainCtx {
            chain_draft: chain_draft.clone(),
            project_session: project_session.clone(),
            chain_editor_window: chain_editor_window.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            chain_input_channels: chain_input_channels.clone(),
            chain_output_channels: chain_output_channels.clone(),
            inline_io_groups_is_input: inline_io_groups_is_input.clone(),
            toast_timer: toast_timer.clone(),
        },
    );
    // --- ChainInputGroupsWindow callbacks (extracted to chain_input_groups_wiring) ---
    crate::chain_input_groups_wiring::wire(
        &window,
        &chain_input_window,
        &chain_input_groups_window,
        crate::chain_input_groups_wiring::ChainInputGroupsCtx {
            chain_draft: chain_draft.clone(),
            project_session: project_session.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            chain_input_channels: chain_input_channels.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            auto_save,
        },
    );
    // --- ChainOutputGroupsWindow callbacks (extracted to chain_output_groups_wiring) ---
    crate::chain_output_groups_wiring::wire(
        &window,
        &chain_output_window,
        &chain_output_groups_window,
        crate::chain_output_groups_wiring::ChainOutputGroupsCtx {
            chain_draft: chain_draft.clone(),
            project_session: project_session.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            chain_output_channels: chain_output_channels.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            auto_save,
        },
    );
    // --- on_select_chain_block (extracted to select_chain_block_callback + block_editor_window_*) ---
    select_chain_block_callback::wire(
        &window,
        &chain_input_groups_window,
        &chain_output_groups_window,
        &chain_insert_window,
        select_chain_block_callback::SelectChainBlockCallbackCtx {
            selected_block: selected_block.clone(),
            block_editor_draft: block_editor_draft.clone(),
            chain_draft: chain_draft.clone(),
            insert_draft: insert_draft.clone(),
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
            insert_send_channels: insert_send_channels.clone(),
            insert_return_channels: insert_return_channels.clone(),
            open_block_windows: open_block_windows.clone(),
            inline_stream_timer: inline_stream_timer.clone(),
            toast_timer: toast_timer.clone(),
            plugin_info_window: plugin_info_window.clone(),
            vst3_editor_handles: vst3_editor_handles.clone(),
            vst3_sample_rate,
            auto_save,
        },
    );
    // --- Chain block CRUD callbacks (extracted to chain_block_crud_wiring) ---
    crate::chain_block_crud_wiring::wire(
        &window,
        &block_editor_window,
        crate::chain_block_crud_wiring::ChainBlockCrudCtx {
            selected_block: selected_block.clone(),
            block_editor_draft: block_editor_draft.clone(),
            block_model_options: block_model_options.clone(),
            filtered_block_model_options: filtered_block_model_options.clone(),
            block_model_option_labels: block_model_option_labels.clone(),
            block_parameter_items: block_parameter_items.clone(),
            multi_slider_points: multi_slider_points.clone(),
            curve_editor_points: curve_editor_points.clone(),
            eq_band_curves: eq_band_curves.clone(),
            block_editor_persist_timer: block_editor_persist_timer.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            toast_timer: toast_timer.clone(),
            open_block_windows: open_block_windows.clone(),
            auto_save,
        },
    );
    // --- on_start_block_insert + on_choose_block_model (extracted to block_insert_callbacks) ---
    block_insert_callbacks::wire(
        &window,
        &block_editor_window,
        block_insert_callbacks::BlockInsertCallbacksCtx {
            selected_block: selected_block.clone(),
            block_editor_draft: block_editor_draft.clone(),
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
            block_editor_persist_timer: block_editor_persist_timer.clone(),
            auto_save,
        },
    );
    // --- on_choose_block_type (extracted to block_choose_type_callback) ---
    block_choose_type_callback::wire(
        &window,
        &block_editor_window,
        &chain_input_window,
        &chain_output_window,
        &chain_insert_window,
        block_choose_type_callback::BlockChooseTypeCallbackCtx {
            block_editor_draft: block_editor_draft.clone(),
            chain_draft: chain_draft.clone(),
            io_block_insert_draft: io_block_insert_draft.clone(),
            insert_draft: insert_draft.clone(),
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
            auto_save,
        },
    );
    // --- Block model search callbacks (extracted to block_model_search_wiring) ---
    crate::block_model_search_wiring::wire(
        &window,
        &block_editor_window,
        block_model_options.clone(),
        filtered_block_model_options.clone(),
    );
    // --- Block picker cancel callback (extracted to block_picker_wiring) ---
    crate::block_picker_wiring::wire(
        &window,
        &block_editor_window,
        crate::block_picker_wiring::BlockPickerCtx {
            block_editor_draft: block_editor_draft.clone(),
            block_model_options: block_model_options.clone(),
            filtered_block_model_options: filtered_block_model_options.clone(),
            block_model_option_labels: block_model_option_labels.clone(),
            block_parameter_items: block_parameter_items.clone(),
            multi_slider_points: multi_slider_points.clone(),
            curve_editor_points: curve_editor_points.clone(),
            eq_band_curves: eq_band_curves.clone(),
            block_editor_persist_timer: block_editor_persist_timer.clone(),
        },
    );
    // --- Block drawer close (extracted to block_drawer_close_wiring) ---
    crate::block_drawer_close_wiring::wire(
        &window,
        &block_editor_window,
        crate::block_drawer_close_wiring::BlockDrawerCloseCtx {
            selected_block: selected_block.clone(),
            block_editor_draft: block_editor_draft.clone(),
            block_model_options: block_model_options.clone(),
            filtered_block_model_options: filtered_block_model_options.clone(),
            block_model_option_labels: block_model_option_labels.clone(),
            block_parameter_items: block_parameter_items.clone(),
            multi_slider_points: multi_slider_points.clone(),
            curve_editor_points: curve_editor_points.clone(),
            eq_band_curves: eq_band_curves.clone(),
            block_editor_persist_timer: block_editor_persist_timer.clone(),
            inline_stream_timer: inline_stream_timer.clone(),
        },
    );
    // --- Block parameter callbacks (extracted to block_parameter_wiring) ---
    crate::block_parameter_wiring::wire(
        &window,
        &block_editor_window,
        crate::block_parameter_wiring::BlockParameterCtx {
            block_editor_draft: block_editor_draft.clone(),
            block_parameter_items: block_parameter_items.clone(),
            block_model_options: block_model_options.clone(),
            block_model_option_labels: block_model_option_labels.clone(),
            eq_band_curves: eq_band_curves.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            block_editor_persist_timer: block_editor_persist_timer.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            auto_save,
        },
    );
    // --- VST3 editor open (extracted to vst3_editor_wiring) ---
    crate::vst3_editor_wiring::wire(
        &window,
        vst3_editor_handles_for_on_open.clone(),
        vst3_sample_rate,
    );
    // --- Block drawer save+delete callbacks (extracted to block_drawer_save_delete_wiring) ---
    crate::block_drawer_save_delete_wiring::wire(
        &window,
        &block_editor_window,
        crate::block_drawer_save_delete_wiring::BlockDrawerSaveDeleteCtx {
            selected_block: selected_block.clone(),
            block_editor_draft: block_editor_draft.clone(),
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
            block_editor_persist_timer: block_editor_persist_timer.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            open_compact_window: open_compact_window.clone(),
            auto_save,
        },
    );
    // --- Block delete confirm/cancel callbacks (extracted to block_delete_wiring) ---
    crate::block_delete_wiring::wire(
        &window,
        &block_editor_window,
        crate::block_delete_wiring::BlockDeleteCtx {
            selected_block: selected_block.clone(),
            block_editor_draft: block_editor_draft.clone(),
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
            toast_timer: toast_timer.clone(),
            auto_save,
        },
    );
    // Fullscreen inline chain editor callbacks — delegate to ChainEditorWindow
    // --- Chain editor delegation forwarders (extracted to chain_editor_forwarders_wiring) ---
    crate::chain_editor_forwarders_wiring::wire(&window, chain_editor_window.clone());
    // --- Fullscreen I/O editor + groups callbacks (extracted to chain_io_fullscreen_callbacks) ---
    chain_io_fullscreen_callbacks::wire(
        &window,
        &chain_input_window,
        &chain_output_window,
        &chain_input_groups_window,
        &chain_output_groups_window,
        chain_io_fullscreen_callbacks::ChainIoFullscreenCallbacksCtx {
            chain_editor_window: chain_editor_window.clone(),
            inline_io_groups_is_input: inline_io_groups_is_input.clone(),
            chain_draft: chain_draft.clone(),
            project_session: project_session.clone(),
            chain_input_device_options: chain_input_device_options.clone(),
            chain_output_device_options: chain_output_device_options.clone(),
            chain_input_channels: chain_input_channels.clone(),
            chain_output_channels: chain_output_channels.clone(),
        },
    );
    // --- on_save_chain + on_cancel_chain (extracted to chain_save_cancel_callbacks) ---
    chain_save_cancel_callbacks::wire(
        &window,
        chain_save_cancel_callbacks::ChainSaveCancelCtx {
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
        },
    );
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

    window.run().map_err(|error| anyhow!(error.to_string()))
}
#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
