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
mod tests {
    use super::{open_cli_project, parse_cli_args_from, SELECT_SELECTED_BLOCK_ID};
    use crate::block_editor::{block_editor_data, block_parameter_items_for_editor};
    use crate::project_ops::build_device_settings_from_gui;
    use domain::ids::BlockId;
    use domain::value_objects::ParameterValue;
    use project::catalog::supported_block_models;
    use project::block::{
        schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, SelectBlock,
    };
    use project::param::ParameterSet;
    use slint::Model;
    #[test]
    fn select_block_editor_uses_selected_option_model() {
        let delay_models = delay_model_ids();
        let first_model = delay_models.first().expect("delay catalog must not be empty");
        let second_model = delay_models.get(1).unwrap_or(first_model);
        let block = select_delay_block("chain:0:block:0", first_model.as_str(), second_model.as_str());
        let editor_data = block_editor_data(&block).expect("select should expose editor data");
        assert!(editor_data.is_select);
        assert_eq!(editor_data.effect_type, "delay");
        assert_eq!(editor_data.model_id, second_model.as_str());
        assert_eq!(editor_data.select_options.len(), 2);
        assert_eq!(
            editor_data.selected_select_option_block_id.as_deref(),
            Some("chain:0:block:0::delay_b")
        );
    }
    #[test]
    fn select_block_editor_includes_active_option_picker() {
        let delay_models = delay_model_ids();
        let first_model = delay_models.first().expect("delay catalog must not be empty");
        let second_model = delay_models.get(1).unwrap_or(first_model);
        let block = select_delay_block("chain:0:block:0", first_model.as_str(), second_model.as_str());
        let editor_data = block_editor_data(&block).expect("select should expose editor data");
        let items = block_parameter_items_for_editor(&editor_data);
        let selector = items
            .iter()
            .find(|item| item.path.as_str() == SELECT_SELECTED_BLOCK_ID)
            .expect("select editor should expose active option picker");
        assert_eq!(selector.option_values.row_count(), 2);
        assert_eq!(selector.selected_option_index, 1);
    }
    fn select_delay_block(id: &str, first_model: &str, second_model: &str) -> AudioBlock {
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId(format!("{id}::delay_b")),
                options: vec![
                    delay_block(format!("{id}::delay_a"), first_model, 120.0),
                    delay_block(format!("{id}::delay_b"), second_model, 240.0),
                ],
            }),
        }
    }
    fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
        let schema = schema_for_block_model("delay", model).expect("delay schema should exist");
        let mut params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("delay defaults should normalize");
        params.insert("time_ms", ParameterValue::Float(time_ms));
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "delay".to_string(),
                model: model.to_string(),
                params,
            }),
        }
    }
    fn delay_model_ids() -> Vec<String> {
        supported_block_models("delay")
            .expect("delay catalog should exist")
            .into_iter()
            .map(|entry| entry.model_id)
            .collect()
    }

    // --- ui_index_to_real_block_index tests ---

    use crate::runtime_lifecycle::ui_index_to_real_block_index;
    use project::block::{InputBlock, InputEntry, OutputBlock, OutputEntry};
    use project::chain::{Chain, ChainInputMode, ChainOutputMode};
    use domain::ids::{ChainId, DeviceId};

    fn test_chain(block_kinds: Vec<AudioBlockKind>) -> Chain {
        Chain {
            id: ChainId("test".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            blocks: block_kinds.into_iter().enumerate().map(|(i, kind)| AudioBlock {
                id: BlockId(format!("block:{}", i)),
                enabled: true,
                kind,
            }).collect(),
        }
    }

    fn input_kind() -> AudioBlockKind {
        AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        })
    }

    fn output_kind() -> AudioBlockKind {
        AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        })
    }

    fn effect_kind(effect_type: &str) -> AudioBlockKind {
        AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.into(),
            model: "test".into(),
            params: ParameterSet::default(),
        })
    }

    #[test]
    fn ui_index_maps_correctly_with_standard_chain() {
        // [Input, Comp, Preamp, Delay, Output]
        // UI sees: [Comp(0), Preamp(1), Delay(2)]
        // Real:    [0=Input, 1=Comp, 2=Preamp, 3=Delay, 4=Output]
        let chain = test_chain(vec![
            input_kind(),
            effect_kind("dynamics"),
            effect_kind("preamp"),
            effect_kind("delay"),
            output_kind(),
        ]);
        assert_eq!(ui_index_to_real_block_index(&chain, 0), 1); // UI 0 = Comp = real 1
        assert_eq!(ui_index_to_real_block_index(&chain, 1), 2); // UI 1 = Preamp = real 2
        assert_eq!(ui_index_to_real_block_index(&chain, 2), 3); // UI 2 = Delay = real 3
    }

    #[test]
    fn ui_index_past_end_returns_before_last_output() {
        let chain = test_chain(vec![
            input_kind(),
            effect_kind("delay"),
            output_kind(),
        ]);
        // UI sees [Delay(0)], asking for UI index 1 (past end) → before Output = real 2
        assert_eq!(ui_index_to_real_block_index(&chain, 1), 2);
    }

    #[test]
    fn ui_index_with_extra_input_in_middle() {
        // [Input, Comp, Input2, Delay, Output]
        // Hidden: first Input (0) and last Output (4)
        // UI sees: [Comp(0), Input2(1), Delay(2)]
        // Real:    [0=Input, 1=Comp, 2=Input2, 3=Delay, 4=Output]
        let chain = test_chain(vec![
            input_kind(),
            effect_kind("dynamics"),
            input_kind(),
            effect_kind("delay"),
            output_kind(),
        ]);
        assert_eq!(ui_index_to_real_block_index(&chain, 0), 1); // Comp
        assert_eq!(ui_index_to_real_block_index(&chain, 1), 2); // Input2
        assert_eq!(ui_index_to_real_block_index(&chain, 2), 3); // Delay
    }

    #[test]
    fn ui_index_with_extra_output_in_middle() {
        // [Input, Comp, Output_mid, Delay, Output]
        // Hidden: first Input (0) and last Output (4)
        // UI sees: [Comp(0), Output_mid(1), Delay(2)]
        let chain = test_chain(vec![
            input_kind(),
            effect_kind("dynamics"),
            output_kind(),
            effect_kind("delay"),
            output_kind(),
        ]);
        assert_eq!(ui_index_to_real_block_index(&chain, 0), 1); // Comp
        assert_eq!(ui_index_to_real_block_index(&chain, 1), 2); // Output_mid (visible!)
        assert_eq!(ui_index_to_real_block_index(&chain, 2), 3); // Delay
    }

    #[test]
    fn ui_index_with_no_io_blocks() {
        // [Comp, Delay] — no I/O blocks at all
        let chain = test_chain(vec![
            effect_kind("dynamics"),
            effect_kind("delay"),
        ]);
        assert_eq!(ui_index_to_real_block_index(&chain, 0), 0);
        assert_eq!(ui_index_to_real_block_index(&chain, 1), 1);
    }

    #[test]
    fn ui_index_with_only_io_blocks() {
        // [Input, Output] — no effect blocks
        let chain = test_chain(vec![
            input_kind(),
            output_kind(),
        ]);
        // UI sees nothing, asking for 0 → before Output = real 1
        assert_eq!(ui_index_to_real_block_index(&chain, 0), 1);
    }

    #[test]
    fn save_input_entries_does_not_move_middle_io_blocks() {
        // Chain: [Input0, Gain, Input1, Delay, Output]
        // Simulates what on_save does: update ONLY first InputBlock entries,
        // verify middle InputBlock at position 2 is untouched.
        let mut chain = test_chain(vec![
            input_kind(),
            effect_kind("gain"),
            AudioBlockKind::Input(InputBlock {
                model: "standard".into(),
                entries: vec![InputEntry {
                    device_id: DeviceId("dev2".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![1],
                }],
            }),
            effect_kind("delay"),
            output_kind(),
        ]);

        // The save path with editing_io_block_index = None finds FIRST InputBlock
        let target_idx = chain.blocks.iter()
            .position(|b| matches!(&b.kind, AudioBlockKind::Input(_)))
            .unwrap();
        assert_eq!(target_idx, 0);

        // Update first InputBlock entries (simulating save)
        let new_entries = vec![InputEntry {
            device_id: DeviceId("dev_new".into()),
            mode: ChainInputMode::Stereo,
            channels: vec![2, 3],
        }];
        if let AudioBlockKind::Input(ref mut ib) = chain.blocks[target_idx].kind {
            ib.entries = new_entries;
        }

        // Verify chain structure is unchanged
        assert_eq!(chain.blocks.len(), 5);
        assert!(matches!(&chain.blocks[0].kind, AudioBlockKind::Input(_)));
        assert!(matches!(&chain.blocks[1].kind, AudioBlockKind::Core(_)));
        assert!(matches!(&chain.blocks[2].kind, AudioBlockKind::Input(_)));
        assert!(matches!(&chain.blocks[3].kind, AudioBlockKind::Core(_)));
        assert!(matches!(&chain.blocks[4].kind, AudioBlockKind::Output(_)));

        // Verify first InputBlock was updated
        if let AudioBlockKind::Input(ref ib) = chain.blocks[0].kind {
            assert_eq!(ib.entries.len(), 1);
            assert_eq!(ib.entries[0].device_id.0, "dev_new");
        } else {
            panic!("block 0 should be Input");
        }

        // Verify middle InputBlock at position 2 is UNTOUCHED
        if let AudioBlockKind::Input(ref ib) = chain.blocks[2].kind {
            assert_eq!(ib.entries.len(), 1);
            assert_eq!(ib.entries[0].device_id.0, "dev2");
            assert_eq!(ib.entries[0].channels, vec![1]);
        } else {
            panic!("block 2 should be Input");
        }

        // Verify block IDs haven't changed (no reconstruction)
        assert_eq!(chain.blocks[0].id.0, "block:0");
        assert_eq!(chain.blocks[1].id.0, "block:1");
        assert_eq!(chain.blocks[2].id.0, "block:2");
        assert_eq!(chain.blocks[3].id.0, "block:3");
        assert_eq!(chain.blocks[4].id.0, "block:4");
    }

    // --- format_channel_list ---

    use super::project_view::format_channel_list;

    #[test]
    fn format_channel_list_empty_returns_dash() {
        assert_eq!(format_channel_list(&[]), "-");
    }

    #[test]
    fn format_channel_list_single_channel_is_one_indexed() {
        assert_eq!(format_channel_list(&[0]), "1");
        assert_eq!(format_channel_list(&[3]), "4");
    }

    #[test]
    fn format_channel_list_multiple_channels_comma_separated() {
        assert_eq!(format_channel_list(&[0, 1]), "1, 2");
        assert_eq!(format_channel_list(&[2, 5, 7]), "3, 6, 8");
    }

    // --- unit_label ---

    use super::block_editor::unit_label;
    use project::param::ParameterUnit;

    #[test]
    fn unit_label_returns_correct_suffix_for_all_variants() {
        assert_eq!(unit_label(&ParameterUnit::None), "");
        assert_eq!(unit_label(&ParameterUnit::Decibels), "dB");
        assert_eq!(unit_label(&ParameterUnit::Hertz), "Hz");
        assert_eq!(unit_label(&ParameterUnit::Milliseconds), "ms");
        assert_eq!(unit_label(&ParameterUnit::Percent), "%");
        assert_eq!(unit_label(&ParameterUnit::Ratio), "Ratio");
        assert_eq!(unit_label(&ParameterUnit::Semitones), "st");
    }

    // --- insert_mode_to_index / insert_mode_from_index ---

    use crate::chain_editor::insert_mode_to_index;
    use crate::chain_editor::insert_mode_from_index;

    #[test]
    fn insert_mode_mono_roundtrip() {
        assert_eq!(insert_mode_to_index(ChainInputMode::Mono), 0);
        assert_eq!(insert_mode_from_index(0), ChainInputMode::Mono);
    }

    #[test]
    fn insert_mode_stereo_roundtrip() {
        assert_eq!(insert_mode_to_index(ChainInputMode::Stereo), 1);
        assert_eq!(insert_mode_from_index(1), ChainInputMode::Stereo);
    }

    #[test]
    fn insert_mode_dual_mono_maps_to_zero() {
        assert_eq!(insert_mode_to_index(ChainInputMode::DualMono), 0);
    }

    #[test]
    fn insert_mode_from_negative_index_defaults_to_mono() {
        assert_eq!(insert_mode_from_index(-1), ChainInputMode::Mono);
    }

    // --- normalized_chain_description ---

    use super::chain_editor::normalized_chain_description;

    #[test]
    fn normalized_chain_description_trims_whitespace() {
        assert_eq!(normalized_chain_description("  Guitar 1  "), Some("Guitar 1".to_string()));
    }

    #[test]
    fn normalized_chain_description_empty_returns_none() {
        assert_eq!(normalized_chain_description(""), None);
        assert_eq!(normalized_chain_description("   "), None);
    }

    // --- preset_id_from_path ---

    use super::project_ops::preset_id_from_path;

    #[test]
    fn preset_id_from_path_extracts_stem() {
        let path = std::path::Path::new("/some/dir/my_preset.yaml");
        assert_eq!(preset_id_from_path(path).unwrap(), "my_preset");
    }

    #[test]
    fn preset_id_from_path_no_extension_uses_filename() {
        let path = std::path::Path::new("/some/dir/my_preset");
        assert_eq!(preset_id_from_path(path).unwrap(), "my_preset");
    }

    // --- project_title_for_path ---

    use super::project_title_for_path;
    use project::project::Project;

    #[test]
    fn project_title_uses_name_when_present() {
        let project = Project {
            name: Some("My Rig".to_string()),
            device_settings: vec![],
            chains: vec![],
        };
        assert_eq!(project_title_for_path(None, &project), "My Rig");
    }

    #[test]
    fn project_title_falls_back_to_path_stem() {
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
        };
        let path = std::path::PathBuf::from("/home/user/my_project.yaml");
        assert_eq!(project_title_for_path(Some(&path), &project), "my_project");
    }

    #[test]
    fn project_title_empty_name_treated_as_absent() {
        let project = Project {
            name: Some("  ".to_string()),
            device_settings: vec![],
            chains: vec![],
        };
        let path = std::path::PathBuf::from("/home/user/fallback.yaml");
        assert_eq!(project_title_for_path(Some(&path), &project), "fallback");
    }

    #[test]
    fn project_title_no_name_no_path_empty_chains_is_novo_projeto() {
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
        };
        assert_eq!(project_title_for_path(None, &project), "Novo Projeto");
    }

    #[test]
    fn project_title_no_name_no_path_with_chains_is_projeto() {
        let chain = Chain {
            id: ChainId("c".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![],
        };
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![chain],
        };
        assert_eq!(project_title_for_path(None, &project), "Projeto");
    }

    // --- selected_device_index ---

    use crate::audio_devices::selected_device_index;
    use infra_cpal::AudioDeviceDescriptor;

    #[test]
    fn selected_device_index_finds_matching_device() {
        let devices = vec![
            AudioDeviceDescriptor { id: "dev_a".into(), name: "A".into(), channels: 2 },
            AudioDeviceDescriptor { id: "dev_b".into(), name: "B".into(), channels: 4 },
        ];
        assert_eq!(selected_device_index(&devices, Some("dev_b")), 1);
    }

    #[test]
    fn selected_device_index_falls_back_to_zero_when_single_device() {
        // When there is exactly one device and the saved ID doesn't match,
        // auto-select it (index 0) so the user doesn't have to manually
        // pick the only option (common on single-device setups like Orange Pi).
        let devices = vec![
            AudioDeviceDescriptor { id: "dev_a".into(), name: "A".into(), channels: 2 },
        ];
        assert_eq!(selected_device_index(&devices, Some("dev_x")), 0);
    }

    #[test]
    fn selected_device_index_returns_negative_when_not_found_multiple_devices() {
        // When there are multiple devices and none match, return -1
        // so the UI shows "Select device" instead of picking one arbitrarily.
        let devices = vec![
            AudioDeviceDescriptor { id: "dev_a".into(), name: "A".into(), channels: 2 },
            AudioDeviceDescriptor { id: "dev_b".into(), name: "B".into(), channels: 4 },
        ];
        assert_eq!(selected_device_index(&devices, Some("dev_x")), -1);
    }

    #[test]
    fn selected_device_index_returns_negative_for_none() {
        let devices = vec![
            AudioDeviceDescriptor { id: "dev_a".into(), name: "A".into(), channels: 2 },
        ];
        assert_eq!(selected_device_index(&devices, None), -1);
    }

    // --- real_block_index_to_ui ---

    use crate::project_view::real_block_index_to_ui;

    #[test]
    fn real_block_index_to_ui_maps_effect_blocks_correctly() {
        // [Input, Comp, Preamp, Delay, Output]
        let chain = test_chain(vec![
            input_kind(),
            effect_kind("dynamics"),
            effect_kind("preamp"),
            effect_kind("delay"),
            output_kind(),
        ]);
        assert_eq!(real_block_index_to_ui(&chain, 1), Some(0));
        assert_eq!(real_block_index_to_ui(&chain, 2), Some(1));
        assert_eq!(real_block_index_to_ui(&chain, 3), Some(2));
    }

    #[test]
    fn real_block_index_to_ui_hidden_blocks_return_none() {
        let chain = test_chain(vec![
            input_kind(),
            effect_kind("delay"),
            output_kind(),
        ]);
        assert_eq!(real_block_index_to_ui(&chain, 0), None); // first input hidden
        assert_eq!(real_block_index_to_ui(&chain, 2), None); // last output hidden
    }

    #[test]
    fn real_block_index_to_ui_out_of_range_returns_none() {
        let chain = test_chain(vec![
            input_kind(),
            output_kind(),
        ]);
        assert_eq!(real_block_index_to_ui(&chain, 99), None);
    }

    // --- project_display_name ---

    use super::{project_display_name, UNTITLED_PROJECT_NAME};

    #[test]
    fn project_display_name_returns_trimmed_name() {
        let project = Project {
            name: Some("  My Project  ".to_string()),
            device_settings: vec![],
            chains: vec![],
        };
        assert_eq!(project_display_name(&project), "My Project");
    }

    #[test]
    fn project_display_name_no_name_returns_untitled() {
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
        };
        assert_eq!(project_display_name(&project), UNTITLED_PROJECT_NAME);
    }

    #[test]
    fn project_display_name_empty_name_returns_untitled() {
        let project = Project {
            name: Some("".to_string()),
            device_settings: vec![],
            chains: vec![],
        };
        assert_eq!(project_display_name(&project), UNTITLED_PROJECT_NAME);
    }

    // --- parse_cli_args_from additional edge cases ---

    #[test]
    fn parse_cli_args_auto_save_before_path() {
        let (path, auto_save, _) = parse_cli_args_from(&["openrig", "--auto-save", "/tmp/p.yaml"]);
        assert_eq!(path, Some(std::path::PathBuf::from("/tmp/p.yaml")));
        assert!(auto_save);
    }

    #[test]
    fn parse_cli_args_multiple_paths_last_wins() {
        let (path, _, _) = parse_cli_args_from(&["openrig", "/first.yaml", "/second.yaml"]);
        assert_eq!(path, Some(std::path::PathBuf::from("/second.yaml")));
    }

    #[test]
    fn parse_cli_args_dashed_flags_ignored_as_paths() {
        let (path, auto_save, fullscreen) = parse_cli_args_from(&["openrig", "--verbose", "--debug"]);
        assert_eq!(path, None);
        assert!(!auto_save);
        assert!(!fullscreen);
    }

    // --- chain_endpoint_label ---

    use super::project_view::chain_endpoint_label;

    #[test]
    fn chain_endpoint_label_returns_prefix() {
        assert_eq!(chain_endpoint_label("In", &[0, 1]), "In");
        assert_eq!(chain_endpoint_label("Out", &[]), "Out");
    }

    #[test]
    fn save_output_entries_finds_last_output_block() {
        // Chain: [Input, Gain, Output_mid, Delay, Output_last]
        // With editing_io_block_index = None, should find LAST OutputBlock
        let mut chain = test_chain(vec![
            input_kind(),
            effect_kind("gain"),
            AudioBlockKind::Output(OutputBlock {
                model: "standard".into(),
                entries: vec![OutputEntry {
                    device_id: DeviceId("dev_mid".into()),
                    mode: ChainOutputMode::Stereo,
                    channels: vec![2, 3],
                }],
            }),
            effect_kind("delay"),
            output_kind(),
        ]);

        // The save path with editing_io_block_index = None finds LAST OutputBlock
        let target_idx = chain.blocks.iter()
            .rposition(|b| matches!(&b.kind, AudioBlockKind::Output(_)))
            .unwrap();
        assert_eq!(target_idx, 4);

        // Update last OutputBlock entries
        let new_entries = vec![OutputEntry {
            device_id: DeviceId("dev_updated".into()),
            mode: ChainOutputMode::Mono,
            channels: vec![0],
        }];
        if let AudioBlockKind::Output(ref mut ob) = chain.blocks[target_idx].kind {
            ob.entries = new_entries;
        }

        // Verify middle OutputBlock at position 2 is UNTOUCHED
        if let AudioBlockKind::Output(ref ob) = chain.blocks[2].kind {
            assert_eq!(ob.entries[0].device_id.0, "dev_mid");
        } else {
            panic!("block 2 should be Output");
        }

        // Verify last OutputBlock was updated
        if let AudioBlockKind::Output(ref ob) = chain.blocks[4].kind {
            assert_eq!(ob.entries[0].device_id.0, "dev_updated");
        } else {
            panic!("block 4 should be Output");
        }
    }

    #[test]
    fn open_cli_project_errors_on_nonexistent_path() {
        let result = open_cli_project(&std::path::PathBuf::from("/nonexistent/project.yaml"));
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("does not exist"), "got: {}", msg);
    }

    #[test]
    fn parse_cli_args_extracts_path_and_auto_save_flag() {
        let (path, auto_save, fullscreen) = parse_cli_args_from(&["openrig", "/tmp/project.yaml"]);
        assert_eq!(path, Some(std::path::PathBuf::from("/tmp/project.yaml")));
        assert!(!auto_save);
        assert!(!fullscreen);

        let (path, auto_save, _) = parse_cli_args_from(&["openrig", "--auto-save"]);
        assert_eq!(path, None);
        assert!(auto_save);

        let (path, auto_save, _) = parse_cli_args_from(&["openrig", "/tmp/project.yaml", "--auto-save"]);
        assert_eq!(path, Some(std::path::PathBuf::from("/tmp/project.yaml")));
        assert!(auto_save);

        let (path, auto_save, _) = parse_cli_args_from(&["openrig"]);
        assert_eq!(path, None);
        assert!(!auto_save);

        let (path, auto_save, _) = parse_cli_args_from(&["openrig", "--unknown-flag"]);
        assert_eq!(path, None);
        assert!(!auto_save);
    }

    #[test]
    fn parse_cli_args_fullscreen_flag() {
        let (_, _, fullscreen) = parse_cli_args_from(&["openrig", "--fullscreen"]);
        assert!(fullscreen);

        let (path, auto_save, fullscreen) = parse_cli_args_from(&["openrig", "--fullscreen", "--auto-save", "/tmp/p.yaml"]);
        assert_eq!(path, Some(std::path::PathBuf::from("/tmp/p.yaml")));
        assert!(auto_save);
        assert!(fullscreen);

        let (_, _, fullscreen) = parse_cli_args_from(&["openrig", "/tmp/p.yaml"]);
        assert!(!fullscreen);
    }

    // --- sync_recent_projects ---

    use super::project_ops::sync_recent_projects;
    use infra_filesystem::{AppConfig, RecentProjectEntry};

    #[test]
    fn sync_recent_projects_deduplicates_by_canonical_path() {
        let mut config = AppConfig {
            recent_projects: vec![
                RecentProjectEntry {
                    project_path: "/tmp/project_a.yaml".to_string(),
                    project_name: "A".to_string(),
                    is_valid: true,
                    invalid_reason: None,
                },
                RecentProjectEntry {
                    project_path: "/tmp/project_a.yaml".to_string(),
                    project_name: "A duplicate".to_string(),
                    is_valid: true,
                    invalid_reason: None,
                },
            ],
            ..Default::default()
        };
        let changed = sync_recent_projects(&mut config);
        assert!(changed);
        assert_eq!(config.recent_projects.len(), 1);
        assert_eq!(config.recent_projects[0].project_name, "A");
    }

    #[test]
    fn sync_recent_projects_empty_name_becomes_untitled() {
        let mut config = AppConfig {
            recent_projects: vec![RecentProjectEntry {
                project_path: "/tmp/x.yaml".to_string(),
                project_name: "  ".to_string(),
                is_valid: true,
                invalid_reason: None,
            }],
            ..Default::default()
        };
        sync_recent_projects(&mut config);
        assert_eq!(config.recent_projects[0].project_name, UNTITLED_PROJECT_NAME);
    }

    #[test]
    fn sync_recent_projects_returns_false_when_unchanged() {
        let mut config = AppConfig {
            recent_projects: vec![RecentProjectEntry {
                project_path: "/tmp/project.yaml".to_string(),
                project_name: "My Project".to_string(),
                is_valid: true,
                invalid_reason: None,
            }],
            ..Default::default()
        };
        let changed = sync_recent_projects(&mut config);
        assert!(!changed);
    }

    // --- register_recent_project ---

    use super::register_recent_project;

    #[test]
    fn register_recent_project_adds_to_front() {
        let mut config = AppConfig {
            recent_projects: vec![RecentProjectEntry {
                project_path: "/old.yaml".to_string(),
                project_name: "Old".to_string(),
                is_valid: true,
                invalid_reason: None,
            }],
            ..Default::default()
        };
        register_recent_project(&mut config, &std::path::PathBuf::from("/new.yaml"), "New");
        assert_eq!(config.recent_projects.len(), 2);
        assert_eq!(config.recent_projects[0].project_name, "New");
    }

    #[test]
    fn register_recent_project_removes_duplicate_and_reinserts_at_front() {
        let path = std::path::PathBuf::from("/project.yaml");
        let mut config = AppConfig {
            recent_projects: vec![
                RecentProjectEntry {
                    project_path: "/other.yaml".to_string(),
                    project_name: "Other".to_string(),
                    is_valid: true,
                    invalid_reason: None,
                },
                RecentProjectEntry {
                    project_path: "/project.yaml".to_string(),
                    project_name: "Project".to_string(),
                    is_valid: true,
                    invalid_reason: None,
                },
            ],
            ..Default::default()
        };
        register_recent_project(&mut config, &path, "Updated");
        assert_eq!(config.recent_projects.len(), 2);
        assert_eq!(config.recent_projects[0].project_name, "Updated");
        assert_eq!(config.recent_projects[1].project_name, "Other");
    }

    #[test]
    fn register_recent_project_empty_name_becomes_untitled() {
        let mut config = AppConfig::default();
        register_recent_project(&mut config, &std::path::PathBuf::from("/x.yaml"), "  ");
        assert_eq!(config.recent_projects[0].project_name, UNTITLED_PROJECT_NAME);
    }

    // --- mark_recent_project_invalid ---

    use crate::project_ops::mark_recent_project_invalid;

    #[test]
    fn mark_recent_project_invalid_sets_flag_and_reason() {
        let mut config = AppConfig {
            recent_projects: vec![RecentProjectEntry {
                project_path: "/p.yaml".to_string(),
                project_name: "P".to_string(),
                is_valid: true,
                invalid_reason: None,
            }],
            ..Default::default()
        };
        mark_recent_project_invalid(&mut config, &std::path::PathBuf::from("/p.yaml"), "File corrupted");
        assert!(!config.recent_projects[0].is_valid);
        assert_eq!(
            config.recent_projects[0].invalid_reason.as_deref(),
            Some("File corrupted")
        );
    }

    #[test]
    fn mark_recent_project_invalid_empty_reason_gets_default() {
        let mut config = AppConfig {
            recent_projects: vec![RecentProjectEntry {
                project_path: "/p.yaml".to_string(),
                project_name: "P".to_string(),
                is_valid: true,
                invalid_reason: None,
            }],
            ..Default::default()
        };
        mark_recent_project_invalid(&mut config, &std::path::PathBuf::from("/p.yaml"), "  ");
        assert!(!config.recent_projects[0].is_valid);
        assert_eq!(
            config.recent_projects[0].invalid_reason.as_deref(),
            Some("Projeto inválido")
        );
    }

    #[test]
    fn mark_recent_project_invalid_nonexistent_path_does_nothing() {
        let mut config = AppConfig {
            recent_projects: vec![RecentProjectEntry {
                project_path: "/p.yaml".to_string(),
                project_name: "P".to_string(),
                is_valid: true,
                invalid_reason: None,
            }],
            ..Default::default()
        };
        mark_recent_project_invalid(&mut config, &std::path::PathBuf::from("/other.yaml"), "err");
        assert!(config.recent_projects[0].is_valid);
        assert!(config.recent_projects[0].invalid_reason.is_none());
    }

    // --- chain_inputs_tooltip ---

    use super::project_view::chain_inputs_tooltip;

    #[test]
    fn chain_inputs_tooltip_shows_device_name_and_channels() {
        let chain = test_chain(vec![
            input_kind(),
            output_kind(),
        ]);
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![chain.clone()],
        };
        let devices = vec![
            AudioDeviceDescriptor { id: "dev".into(), name: "USB Audio".into(), channels: 2 },
        ];
        let tooltip = chain_inputs_tooltip(&chain, &project, &devices);
        assert!(tooltip.contains("USB Audio"), "tooltip should contain device name: {}", tooltip);
        assert!(tooltip.contains("Mono"), "tooltip should contain mode: {}", tooltip);
        assert!(tooltip.contains("1"), "tooltip should contain channel number: {}", tooltip);
    }

    #[test]
    fn chain_inputs_tooltip_no_input_block() {
        let chain = test_chain(vec![
            effect_kind("delay"),
        ]);
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
        };
        let tooltip = chain_inputs_tooltip(&chain, &project, &[]);
        assert_eq!(tooltip, "No input configured");
    }

    #[test]
    fn chain_inputs_tooltip_unknown_device_shows_id() {
        let chain = test_chain(vec![
            input_kind(),
            output_kind(),
        ]);
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
        };
        // No devices → falls back to device_id
        let tooltip = chain_inputs_tooltip(&chain, &project, &[]);
        assert!(tooltip.contains("dev"), "should fall back to device id: {}", tooltip);
    }

    // --- chain_outputs_tooltip ---

    use super::project_view::chain_outputs_tooltip;

    #[test]
    fn chain_outputs_tooltip_shows_device_and_channels() {
        let chain = test_chain(vec![
            input_kind(),
            output_kind(),
        ]);
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![chain.clone()],
        };
        let devices = vec![
            AudioDeviceDescriptor { id: "dev".into(), name: "Headphones".into(), channels: 2 },
        ];
        let tooltip = chain_outputs_tooltip(&chain, &project, &devices);
        assert!(tooltip.contains("Headphones"), "should contain device name: {}", tooltip);
        assert!(tooltip.contains("Stereo"), "should contain mode: {}", tooltip);
    }

    #[test]
    fn chain_outputs_tooltip_no_output_block() {
        let chain = test_chain(vec![
            effect_kind("delay"),
        ]);
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
        };
        let tooltip = chain_outputs_tooltip(&chain, &project, &[]);
        assert_eq!(tooltip, "No output configured");
    }

    // --- block_model_index ---

    use crate::project_view::block_model_index;

    #[test]
    fn block_model_index_finds_known_delay_model() {
        let models = delay_model_ids();
        let first = &models[0];
        let idx = block_model_index("delay", first, "electric_guitar");
        assert_eq!(idx, 0);
    }

    #[test]
    fn block_model_index_unknown_model_returns_negative() {
        let idx = block_model_index("delay", "nonexistent_model", "electric_guitar");
        assert_eq!(idx, -1);
    }

    // --- block_type_index ---

    use crate::project_view::block_type_index;

    #[test]
    fn block_type_index_finds_delay() {
        let idx = block_type_index("delay", "electric_guitar");
        assert!(idx >= 0, "delay should be in type picker");
    }

    #[test]
    fn block_type_index_unknown_type_returns_negative() {
        let idx = block_type_index("nonexistent_type", "electric_guitar");
        assert_eq!(idx, -1);
    }

    #[test]
    fn block_type_index_input_is_present() {
        let idx = block_type_index("input", "electric_guitar");
        assert!(idx >= 0, "input should be in type picker");
    }

    #[test]
    fn block_type_index_output_is_present() {
        let idx = block_type_index("output", "electric_guitar");
        assert!(idx >= 0, "output should be in type picker");
    }

    #[test]
    fn block_type_index_insert_is_present() {
        let idx = block_type_index("insert", "electric_guitar");
        assert!(idx >= 0, "insert should be in type picker");
    }

    #[test]
    fn build_device_settings_deduplicates_same_device_id() {
        use infra_filesystem::GuiAudioDeviceSettings;
        let input = vec![GuiAudioDeviceSettings {
            device_id: "alsa:hw:CARD=Q26,DEV=0".into(),
            name: "TEYUN Q26".into(),
            sample_rate: 48000,
            buffer_size_frames: 64,
            bit_depth: 16,
            ..GuiAudioDeviceSettings::default()
        }];
        let output = vec![GuiAudioDeviceSettings {
            device_id: "alsa:hw:CARD=Q26,DEV=0".into(),
            name: "TEYUN Q26".into(),
            sample_rate: 48000,
            buffer_size_frames: 64,
            bit_depth: 16,
            ..GuiAudioDeviceSettings::default()
        }];
        let result = build_device_settings_from_gui(&input, &output);
        assert_eq!(result.len(), 1, "same device_id in input+output should produce 1 entry");
        assert_eq!(result[0].device_id.0, "alsa:hw:CARD=Q26,DEV=0");
    }

    #[test]
    fn build_device_settings_keeps_distinct_devices() {
        use infra_filesystem::GuiAudioDeviceSettings;
        let input = vec![GuiAudioDeviceSettings {
            device_id: "alsa:hw:CARD=Q26,DEV=0".into(),
            name: "TEYUN Q26".into(),
            sample_rate: 48000,
            buffer_size_frames: 64,
            bit_depth: 16,
            ..GuiAudioDeviceSettings::default()
        }];
        let output = vec![GuiAudioDeviceSettings {
            device_id: "alsa:hw:CARD=hdmi0,DEV=0".into(),
            name: "HDMI".into(),
            sample_rate: 48000,
            buffer_size_frames: 128,
            bit_depth: 24,
            ..GuiAudioDeviceSettings::default()
        }];
        let result = build_device_settings_from_gui(&input, &output);
        assert_eq!(result.len(), 2, "different device_ids should produce 2 entries");
    }

    #[test]
    fn build_device_settings_input_takes_precedence_on_duplicate() {
        use infra_filesystem::GuiAudioDeviceSettings;
        let input = vec![GuiAudioDeviceSettings {
            device_id: "alsa:hw:CARD=Q26,DEV=0".into(),
            name: "TEYUN Q26".into(),
            sample_rate: 48000,
            buffer_size_frames: 128,
            bit_depth: 24,
            ..GuiAudioDeviceSettings::default()
        }];
        let output = vec![GuiAudioDeviceSettings {
            device_id: "alsa:hw:CARD=Q26,DEV=0".into(),
            name: "TEYUN Q26".into(),
            sample_rate: 44100,
            buffer_size_frames: 64,
            bit_depth: 16,
            ..GuiAudioDeviceSettings::default()
        }];
        let result = build_device_settings_from_gui(&input, &output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].sample_rate, 48000, "input settings should take precedence");
        assert_eq!(result[0].buffer_size_frames, 128);
    }
}