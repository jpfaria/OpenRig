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
pub(crate) use chain_editor_callbacks::setup_chain_editor_callbacks;

use anyhow::{anyhow, Result};

const SELECT_PATH_PREFIX: &str = "__select.";
const SELECT_SELECTED_BLOCK_ID: &str = "__select.selected_block_id";
use application::validate::validate_project;
use domain::ids::{BlockId, ChainId};
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use infra_filesystem::FilesystemStorage;
use project::block::{AudioBlock, AudioBlockKind};
use project::catalog::{model_brand, model_display_name, model_type_label};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use rfd::FileDialog;
use slint::{Model, ModelRc, SharedString, Timer, VecModel};

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use ui_openrig::{AppRuntimeMode, InteractionMode, UiRuntimeContext};
use ui_state::block_drawer_state;
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
    ProjectSession, InputGroupDraft, OutputGroupDraft,
    ChainDraft, SelectedBlock, BlockEditorDraft, IoBlockInsertDraft, InsertDraft,
    AudioSettingsMode, UNTITLED_PROJECT_NAME, BlockWindow,
};
use audio_devices::{
    refresh_input_devices, refresh_output_devices,
    selected_device_index, build_project_device_rows, build_input_channel_items,
    build_output_channel_items, replace_channel_options, build_insert_send_channel_items,
    build_insert_return_channel_items,
    default_device_settings, normalize_device_settings, mark_unselected_devices,
};
use block_editor::{
    block_editor_data, block_parameter_extensions,
    block_parameter_items_for_editor, block_parameter_items_for_model, build_knob_overlays,
    build_params_from_items, persist_block_editor_draft,
    schedule_block_editor_persist, schedule_block_editor_persist_for_block_win,
    set_block_parameter_bool, set_block_parameter_number, set_block_parameter_option,
    set_block_parameter_text,
};
use chain_editor::{
    chain_draft_from_chain, chain_from_draft,
    input_mode_to_index, output_mode_to_index,
    insert_mode_to_index,
};
use eq::{compute_eq_curves, build_multi_slider_points, build_curve_editor_points};
use helpers::{
    show_child_window, use_inline_block_editor,
    set_status_error, set_status_info, set_status_warning,
    clear_status, sync_block_editor_window,
};
use io_groups::{
    build_io_group_items, apply_chain_input_window_state,
    apply_chain_output_window_state,
};
use project_ops::{
    open_cli_project, resolve_project_paths, load_and_sync_app_config,
    canonical_project_path, register_recent_project,
    recent_project_items, project_display_name,
    project_session_snapshot,
    set_project_dirty, sync_project_dirty,
    project_title_for_path,
};
use project_view::{
    block_type_picker_items,
    block_model_picker_items, block_model_picker_labels, set_selected_block, block_type_index,
    block_model_index_from_items, block_model_index,
    load_screenshot_image,
    replace_project_chains,
};
const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const DEFAULT_BUFFER_SIZE_FRAMES: u32 = 64;
const DEFAULT_BIT_DEPTH: u32 = 32;
const SUPPORTED_SAMPLE_RATES: &[u32] = &[44_100, 48_000, 88_200, 96_000];
const SUPPORTED_BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];
const SUPPORTED_BIT_DEPTHS: &[u32] = &[16, 24, 32];

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
    {
        let weak_main_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_model_options = block_model_options.clone();
        let filtered_block_model_options = filtered_block_model_options.clone();
        let block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let multi_slider_points = multi_slider_points.clone();
        let curve_editor_points = curve_editor_points.clone();
        let eq_band_curves = eq_band_curves.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_input_device_options_for_select = chain_input_device_options.clone();
        let chain_output_device_options_for_select = chain_output_device_options.clone();
        let open_block_windows = open_block_windows.clone();
        let inline_stream_timer = inline_stream_timer.clone();
        let toast_timer = toast_timer.clone();
        let weak_input_groups_for_select = chain_input_groups_window.as_weak();
        let weak_output_groups_for_select = chain_output_groups_window.as_weak();
        let chain_draft_for_select = chain_draft.clone();
        let weak_insert_window_for_select = chain_insert_window.as_weak();
        let insert_draft_for_select = insert_draft.clone();
        let insert_send_channels_for_select = insert_send_channels.clone();
        let insert_return_channels_for_select = insert_return_channels.clone();
        let block_type_options_for_select = block_type_options.clone();
        let vst3_handles_for_select = vst3_editor_handles.clone();
        let vst3_sr_for_select = vst3_sample_rate;
        window.on_select_chain_block(move |chain_index, ui_block_index| {
            let Some(window) = weak_main_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let Some(chain) = session.project.chains.get(chain_index as usize) else {
                set_status_error(&window, &toast_timer, "Chain inválida.");
                return;
            };
            // Convert UI index (position in filtered array without first Input/last Output)
            // to real index in chain.blocks — always computed from current chain state
            let block_index = ui_index_to_real_block_index(chain, ui_block_index as usize) as i32;
            log::info!("[select_chain_block] ui_index={} → real_index={}", ui_block_index, block_index);
            let Some(block) = chain.blocks.get(block_index as usize) else {
                log::warn!("[select_chain_block] block_index={} out of bounds, chain has {} blocks", block_index, chain.blocks.len());
                set_status_error(&window, &toast_timer, "Block inválido.");
                return;
            };
            // Handle I/O blocks — open I/O groups window with entries of THIS specific block
            match &block.kind {
                AudioBlockKind::Input(ib) => {
                    let fresh_input = refresh_input_devices(&chain_input_device_options_for_select);
                    let fresh_output = refresh_output_devices(&chain_output_device_options_for_select);
                    let inputs: Vec<InputGroupDraft> = ib.entries.iter().map(|e| InputGroupDraft {
                        device_id: if e.device_id.0.is_empty() { None } else { Some(e.device_id.0.clone()) },
                        channels: e.channels.clone(),
                        mode: e.mode,
                    }).collect();
                    let mut draft = chain_draft_from_chain(chain_index as usize, chain);
                    draft.inputs = inputs;
                    draft.editing_io_block_index = Some(block_index as usize);
                    let (input_items, _) = build_io_group_items(&draft, &fresh_input, &fresh_output);
                    if let Some(gw) = weak_input_groups_for_select.upgrade() {
                        gw.set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
                        gw.set_status_message("".into());
                        gw.set_show_block_controls(true);
                        gw.set_block_enabled(block.enabled);
                        *chain_draft_for_select.borrow_mut() = Some(draft);
                        drop(session_borrow);
                        show_child_window(window.window(), gw.window());
                    }
                    return;
                }
                AudioBlockKind::Output(ob) => {
                    let fresh_input = refresh_input_devices(&chain_input_device_options_for_select);
                    let fresh_output = refresh_output_devices(&chain_output_device_options_for_select);
                    let outputs: Vec<OutputGroupDraft> = ob.entries.iter().map(|e| OutputGroupDraft {
                        device_id: if e.device_id.0.is_empty() { None } else { Some(e.device_id.0.clone()) },
                        channels: e.channels.clone(),
                        mode: e.mode,
                    }).collect();
                    let mut draft = chain_draft_from_chain(chain_index as usize, chain);
                    draft.outputs = outputs;
                    draft.editing_io_block_index = Some(block_index as usize);
                    let (_, output_items) = build_io_group_items(&draft, &fresh_input, &fresh_output);
                    if let Some(gw) = weak_output_groups_for_select.upgrade() {
                        gw.set_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
                        gw.set_status_message("".into());
                        gw.set_show_block_controls(true);
                        gw.set_block_enabled(block.enabled);
                        *chain_draft_for_select.borrow_mut() = Some(draft);
                        drop(session_borrow);
                        show_child_window(window.window(), gw.window());
                    }
                    return;
                }
                AudioBlockKind::Insert(ib) => {
                    let fresh_input = refresh_input_devices(&chain_input_device_options_for_select);
                    let fresh_output = refresh_output_devices(&chain_output_device_options_for_select);
                    log::info!("[select_chain_block] insert block at index {}: id='{}'", block_index, block.id.0);
                    let draft = InsertDraft {
                        chain_index: chain_index as usize,
                        block_index: block_index as usize,
                        send_device_id: if ib.send.device_id.0.is_empty() { None } else { Some(ib.send.device_id.0.clone()) },
                        send_channels: ib.send.channels.clone(),
                        send_mode: ib.send.mode,
                        return_device_id: if ib.return_.device_id.0.is_empty() { None } else { Some(ib.return_.device_id.0.clone()) },
                        return_channels: ib.return_.channels.clone(),
                        return_mode: ib.return_.mode,
                    };
                    let is_middle = block_index > 0 && (block_index as usize) < chain.blocks.len() - 1;
                    if let Some(iw) = weak_insert_window_for_select.upgrade() {
                        let send_items = build_insert_send_channel_items(&draft, &fresh_output);
                        let return_items = build_insert_return_channel_items(&draft, &fresh_input);
                        replace_channel_options(&insert_send_channels_for_select, send_items);
                        replace_channel_options(&insert_return_channels_for_select, return_items);
                        iw.set_selected_send_device_index(selected_device_index(
                            &fresh_output,
                            draft.send_device_id.as_deref(),
                        ));
                        iw.set_selected_return_device_index(selected_device_index(
                            &fresh_input,
                            draft.return_device_id.as_deref(),
                        ));
                        iw.set_selected_send_mode_index(insert_mode_to_index(draft.send_mode));
                        iw.set_selected_return_mode_index(insert_mode_to_index(draft.return_mode));
                        iw.set_show_block_controls(is_middle);
                        iw.set_block_enabled(block.enabled);
                        iw.set_status_message("".into());
                        *insert_draft_for_select.borrow_mut() = Some(draft);
                        drop(session_borrow);
                        show_child_window(window.window(), iw.window());
                    }
                    return;
                }
                _ => {}
            }
            log::info!("[select_chain_block] block at real_index={}: id='{}', kind={}", block_index, block.id.0, block.model_ref().map(|m| format!("{}/{}", m.effect_type, m.model)).unwrap_or_else(|| "io/insert".to_string()));
            log::info!("[select_chain_block] chain has {} blocks:", chain.blocks.len());
            for (i, b) in chain.blocks.iter().enumerate() {
                log::info!("[select_chain_block]   [{}] id='{}' kind={}", i, b.id.0, b.model_ref().map(|m| format!("{}/{}", m.effect_type, m.model)).unwrap_or_else(|| "io/insert".to_string()));
            }
            let Some(editor_data) = block_editor_data(block) else {
                set_status_error(&window, &toast_timer, "Esse block ainda não pode ser editado pela GUI.");
                return;
            };
            let effect_type = editor_data.effect_type.clone();
            let model_id = editor_data.model_id.clone();
            let enabled = editor_data.enabled;
            *selected_block.borrow_mut() = Some(SelectedBlock {
                chain_index: chain_index as usize,
                block_index: block_index as usize,
            });
            let instrument = chain.instrument.clone();
            log::info!("[select_chain_block] chain_index={}, block_index={}, effect_type='{}', model_id='{}', enabled={}", chain_index, block_index, effect_type, model_id, enabled);
            *block_editor_draft.borrow_mut() = Some(BlockEditorDraft {
                chain_index: chain_index as usize,
                block_index: Some(block_index as usize),
                before_index: block_index as usize,
                instrument: instrument.clone(),
                effect_type: effect_type.clone(),
                model_id: model_id.clone(),
                enabled,
                is_select: editor_data.is_select,
            });
            let items = block_model_picker_items(&effect_type, &instrument);
            log::debug!("[select_chain_block] filtered models count={}", items.len());
            for item in &items {
                log::trace!("[select_chain_block]   model='{}'", item.model_id);
            }
            block_model_option_labels.set_vec(block_model_picker_labels(&items));
            block_model_options.set_vec(items.clone());
            filtered_block_model_options.set_vec(items);
            block_parameter_items.set_vec(block_parameter_items_for_editor(&editor_data));
            multi_slider_points.set_vec(build_multi_slider_points(&editor_data.effect_type, &editor_data.model_id, &editor_data.params));
            curve_editor_points.set_vec(build_curve_editor_points(&editor_data.effect_type, &editor_data.model_id, &editor_data.params));
            let (eq_total, eq_bands) = compute_eq_curves(&editor_data.effect_type, &editor_data.model_id, &editor_data.params);
            eq_band_curves.set_vec(eq_bands.into_iter().map(SharedString::from).collect::<Vec<_>>());
            window.set_eq_total_curve(eq_total.into());
            set_selected_block(&window, selected_block.borrow().as_ref(), Some(chain));
            let drawer_state =
                block_drawer_state(Some(block_index as usize), &effect_type, Some(&model_id));
            window.set_block_drawer_title(drawer_state.title.into());
            window.set_block_drawer_confirm_label(drawer_state.confirm_label.into());
            window.set_block_drawer_edit_mode(true);
            block_type_options_for_select.set_vec(block_type_picker_items(&instrument));
            window.set_block_drawer_selected_type_index(block_type_index(&effect_type, &instrument));
            window
                .set_block_drawer_selected_model_index(block_model_index(&effect_type, &model_id, &instrument));
            window.set_block_drawer_enabled(enabled);
            window.set_block_drawer_status_message("".into());
            window.set_show_block_type_picker(false);
            // Clone block_id before dropping session_borrow (needed by window editor stream timer)
            let block_id_for_editor = block.id.clone();
            let is_vst3_block = effect_type == block_core::EFFECT_TYPE_VST3;
            drop(session_borrow);
            // VST3 blocks: open the native plugin GUI directly — no Slint editor popup.
            if is_vst3_block && !model_id.is_empty() {
                match project::vst3_editor::open_vst3_editor(&model_id, vst3_sr_for_select) {
                    Ok(handle) => { vst3_handles_for_select.borrow_mut().push(handle); }
                    Err(e) => set_status_error(&window, &toast_timer, &format!("Erro ao abrir plugin VST3: {}", e)),
                }
                return;
            }
            if use_inline_block_editor(&window) {
                let param_items_vec = block_parameter_items_for_editor(&editor_data);
                let overlays = build_knob_overlays(project::catalog::model_knob_layout(&effect_type, &model_id), &param_items_vec);
                window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
                // Start inline stream timer for utility blocks (tuner, spectrum analyzer)
                {
                    let mut timer_ref = inline_stream_timer.borrow_mut();
                    *timer_ref = None; // stop previous timer
                    let is_utility = effect_type == block_core::EFFECT_TYPE_UTILITY;
                    if is_utility {
                        let timer = Timer::default();
                        let weak_win = window.as_weak();
                        let runtime = project_runtime.clone();
                        let bid = block_id_for_editor.clone();
                        timer.start(
                            slint::TimerMode::Repeated,
                            std::time::Duration::from_millis(50),
                            move || {
                                let Some(win) = weak_win.upgrade() else { return; };
                                let runtime_borrow = runtime.borrow();
                                // No utility block currently produces a "spectrum" stream
                                // (the spectrum_analyzer block was promoted to a top-bar
                                // feature in #320). Kept generic for future stream blocks.
                                let kind: slint::SharedString = "stream".into();
                                let Some(rt) = runtime_borrow.as_ref() else { return; };
                                if let Some(entries) = rt.poll_stream(&bid) {
                                    let slint_entries: Vec<BlockStreamEntry> = entries.iter().map(|e| BlockStreamEntry {
                                        key: e.key.clone().into(),
                                        value: e.value,
                                        text: e.text.clone().into(),
                                        peak: e.peak,
                                    }).collect();
                                    win.set_block_stream_data(BlockStreamData {
                                        active: true,
                                        stream_kind: kind,
                                        entries: ModelRc::from(Rc::new(VecModel::from(slint_entries))),
                                    });
                                } else {
                                    win.set_block_stream_data(BlockStreamData {
                                        active: false,
                                        stream_kind: kind,
                                        entries: ModelRc::default(),
                                    });
                                }
                            },
                        );
                        *timer_ref = Some(timer);
                    }
                }
                window.set_show_block_drawer(true);
            } else {
                window.set_show_block_drawer(false);
                let ci = chain_index as usize;
                let bi = block_index as usize;
                // If this block already has an open editor, bring it to front.
                {
                    let borrow = open_block_windows.borrow();
                    if let Some(bw) = borrow.iter().find(|bw| bw.chain_index == ci && bw.block_index == bi) {
                        show_child_window(window.window(), bw.window.window());
                        return;
                    }
                }
                // Close any stale window for this block position before creating a fresh one.
                // After add/remove operations the block at a given index may have changed.
                {
                    let borrow = open_block_windows.borrow();
                    for bw in borrow.iter().filter(|bw| bw.chain_index == ci && bw.block_index == bi) {
                        let _ = bw.window.hide();
                    }
                }
                open_block_windows.borrow_mut().retain(|bw| {
                    !(bw.chain_index == ci && bw.block_index == bi)
                });
                // Create new isolated window
                let win = match BlockEditorWindow::new() {
                    Ok(w) => w,
                    Err(e) => {
                        set_status_error(&window, &toast_timer, &format!("Erro ao abrir editor: {e}"));
                        return;
                    }
                };
                // Per-window models (independent copies of the data)
                let win_model_options = Rc::new(VecModel::from(
                    block_model_picker_items(&effect_type, &instrument)
                ));
                // Filtered list starts as a copy of the full list so the
                // popup shows everything when first opened. The search
                // callback below replaces it on every keystroke.
                let win_filtered_model_options = Rc::new(VecModel::from(
                    block_model_picker_items(&effect_type, &instrument)
                ));
                let win_model_labels = Rc::new(VecModel::from(
                    block_model_picker_labels(&block_model_picker_items(&effect_type, &instrument))
                ));
                let win_param_items_vec = block_parameter_items_for_editor(&editor_data);
                let win_knob_overlays = Rc::new(VecModel::from(
                    build_knob_overlays(project::catalog::model_knob_layout(&effect_type, &model_id), &win_param_items_vec)
                ));
                let win_param_items = Rc::new(VecModel::from(win_param_items_vec));
                let win_draft = Rc::new(RefCell::new(Some(BlockEditorDraft {
                    chain_index: ci,
                    block_index: Some(bi),
                    before_index: bi,
                    instrument: instrument.clone(),
                    effect_type: effect_type.clone(),
                    model_id: model_id.clone(),
                    enabled,
                    is_select: editor_data.is_select,
                })));
                let win_timer = Rc::new(Timer::default());
                // Populate window — ALL data set independently (no sync from AppWindow)
                let type_index = block_type_index(&effect_type, &instrument);
                let model_index = block_model_index_from_items(&win_model_options, &model_id);
                win.set_block_type_options(ModelRc::from(Rc::new(VecModel::from(block_type_picker_items(&instrument)))));
                win.set_block_model_options(ModelRc::from(win_model_options.clone()));
                win.set_filtered_block_model_options(ModelRc::from(win_filtered_model_options.clone()));
                win.set_block_model_option_labels(ModelRc::from(win_model_labels.clone()));
                crate::model_search_wiring::wire_standalone_block_editor_window(
                    &win,
                    win_model_options.clone(),
                    win_filtered_model_options.clone(),
                );
                win.set_block_parameter_items(ModelRc::from(win_param_items.clone()));
                win.set_block_knob_overlays(ModelRc::from(win_knob_overlays.clone()));
                let win_multi_slider_pts = Rc::new(VecModel::from(
                    build_multi_slider_points(&effect_type, &model_id, &editor_data.params)
                ));
                let win_curve_editor_pts = Rc::new(VecModel::from(
                    build_curve_editor_points(&effect_type, &model_id, &editor_data.params)
                ));
                let (win_eq_total, win_eq_bands) = compute_eq_curves(&effect_type, &model_id, &editor_data.params);
                let win_eq_band_curves = Rc::new(VecModel::from(
                    win_eq_bands.into_iter().map(SharedString::from).collect::<Vec<_>>()
                ));
                win.set_multi_slider_points(ModelRc::from(win_multi_slider_pts.clone()));
                win.set_curve_editor_points(ModelRc::from(win_curve_editor_pts.clone()));
                win.set_eq_total_curve(win_eq_total.into());
                win.set_eq_band_curves(ModelRc::from(win_eq_band_curves.clone()));
                win.set_block_drawer_selected_type_index(type_index);
                win.set_block_drawer_selected_model_index(model_index);
                win.set_block_drawer_edit_mode(true);
                win.set_block_drawer_enabled(enabled);
                win.set_block_drawer_status_message("".into());
                // Set window title
                let title_label = win_model_options
                    .row_data(model_index as usize)
                    .map(|m| m.label.to_string())
                    .unwrap_or_else(|| "Block".to_string());
                win.set_block_window_title(format!("OpenRig · {}", title_label).into());

                // Stream data timer — polls stream data when block produces it (e.g. tuner).
                // Start the timer regardless of current enabled state: when the user enables
                // the block while the popup is open, stream data must appear without reopening.
                let mut block_stream_timer: Option<Rc<Timer>> = None;
                let is_utility = effect_type == block_core::EFFECT_TYPE_UTILITY;
                log::info!("[block-editor-stream] block='{}' effect_type='{}' model='{}' enabled={} is_utility={}", block_id_for_editor.0, effect_type, model_id, enabled, is_utility);
                if is_utility {
                    log::info!("[block-editor-stream] starting stream timer for block '{}'", block_id_for_editor.0);
                    let stream_timer = Rc::new(Timer::default());
                    let weak_win_stream = win.as_weak();
                    let project_runtime_stream = project_runtime.clone();
                    let block_id_for_stream = block_id_for_editor.clone();
                    let mut poll_count: u32 = 0;
                    stream_timer.start(
                        slint::TimerMode::Repeated,
                        std::time::Duration::from_millis(50),
                        move || {
                            let Some(win) = weak_win_stream.upgrade() else { return; };
                            let runtime_borrow = project_runtime_stream.borrow();
                            // No utility block currently produces a "spectrum" stream
                            // (the spectrum_analyzer block was promoted to a top-bar
                            // feature in #320). Kept generic for future stream blocks.
                            let kind: slint::SharedString = "stream".into();
                            let Some(runtime) = runtime_borrow.as_ref() else {
                                poll_count += 1;
                                if poll_count % 40 == 0 {
                                    log::debug!("[block-editor-stream] runtime not available (poll #{})", poll_count);
                                }
                                return;
                            };
                            if let Some(entries) = runtime.poll_stream(&block_id_for_stream) {
                                let slint_entries: Vec<BlockStreamEntry> = entries.iter().map(|e| BlockStreamEntry {
                                    key: e.key.clone().into(),
                                    value: e.value,
                                    text: e.text.clone().into(),
                                    peak: e.peak,
                                }).collect();
                                poll_count += 1;
                                if poll_count % 40 == 1 {
                                    log::debug!("[block-editor-stream] poll #{}: {} entries, first={:?}", poll_count, slint_entries.len(), entries.first().map(|e| &e.key));
                                }
                                win.set_block_stream_data(BlockStreamData {
                                    active: true,
                                    stream_kind: kind,
                                    entries: ModelRc::from(Rc::new(VecModel::from(slint_entries))),
                                });
                            } else {
                                poll_count += 1;
                                if poll_count % 40 == 0 {
                                    log::debug!("[block-editor-stream] poll #{}: no entries (silence or no runtime handle)", poll_count);
                                }
                                win.set_block_stream_data(BlockStreamData {
                                    active: false,
                                    stream_kind: kind.clone(),
                                    entries: ModelRc::default(),
                                });
                            }
                        },
                    );
                    block_stream_timer = Some(stream_timer);
                }

                // on_choose_block_model
                {
                    let win_draft = win_draft.clone();
                    let win_param_items = win_param_items.clone();
                    let win_knob_overlays = win_knob_overlays.clone();
                    let win_multi_slider_pts = win_multi_slider_pts.clone();
                    let win_curve_editor_pts = win_curve_editor_pts.clone();
                    let win_eq_band_curves = win_eq_band_curves.clone();
                    let win_timer = win_timer.clone();
                    let project_session = project_session.clone();
                    let project_chains = project_chains.clone();
                    let project_runtime = project_runtime.clone();
                    let saved_project_snapshot = saved_project_snapshot.clone();
                    let project_dirty = project_dirty.clone();
                    let input_chain_devices = input_chain_devices.clone();
                    let output_chain_devices = output_chain_devices.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_choose_block_model(move |index| {
                        let Some(win) = weak_win.upgrade() else { return; };
                        let mut draft_borrow = win_draft.borrow_mut();
                        let Some(draft) = draft_borrow.as_mut() else { return; };
                        let models = block_model_picker_items(&draft.effect_type, &draft.instrument);
                        let Some(model) = models.get(index as usize) else { return; };
                        draft.model_id = model.model_id.to_string();
                        draft.effect_type = model.effect_type.to_string();
                        let new_params = block_parameter_items_for_model(
                            &model.effect_type, &model.model_id, &ParameterSet::default(),
                        );
                        let overlays = build_knob_overlays(project::catalog::model_knob_layout(&model.effect_type, &model.model_id), &new_params);
                        win_knob_overlays.set_vec(overlays);
                        win_param_items.set_vec(new_params);
                        // Update EQ widgets for the new model
                        let default_params = build_params_from_items(&win_param_items);
                        win_multi_slider_pts.set_vec(build_multi_slider_points(&model.effect_type, &model.model_id, &default_params));
                        win_curve_editor_pts.set_vec(build_curve_editor_points(&model.effect_type, &model.model_id, &default_params));
                        let (eq_total, eq_bands) = compute_eq_curves(&model.effect_type, &model.model_id, &default_params);
                        win_eq_band_curves.set_vec(eq_bands.into_iter().map(SharedString::from).collect::<Vec<_>>());
                        win.set_eq_total_curve(eq_total.into());
                        drop(draft_borrow);
                        if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                            schedule_block_editor_persist_for_block_win(
                                &win_timer, weak_win.clone(), weak_main.clone(),
                                win_draft.clone(), win_param_items.clone(),
                                project_session.clone(), project_chains.clone(), project_runtime.clone(),
                                saved_project_snapshot.clone(), project_dirty.clone(),
                                input_chain_devices.clone(), output_chain_devices.clone(),
                                "block-window.choose-model",
                            auto_save,
                            );
                        }
                    });
                }
                // on_toggle_block_drawer_enabled
                {
                    let win_draft = win_draft.clone();
                    let project_session = project_session.clone();
                    let project_chains = project_chains.clone();
                    let project_runtime = project_runtime.clone();
                    let saved_project_snapshot = saved_project_snapshot.clone();
                    let project_dirty = project_dirty.clone();
                    let input_chain_devices = input_chain_devices.clone();
                    let output_chain_devices = output_chain_devices.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_toggle_block_drawer_enabled(move || {
                        let Some(win) = weak_win.upgrade() else { return; };
                        let Some(main) = weak_main.upgrade() else { return; };
                        let (chain_idx, block_idx, chain_id_opt) = {
                            let (chain_index, block_index) = {
                                let draft_borrow = win_draft.borrow();
                                let Some(draft) = draft_borrow.as_ref() else { return; };
                                let Some(bi) = draft.block_index else { return; };
                                (draft.chain_index, bi)
                            };
                            let mut session_borrow = project_session.borrow_mut();
                            let Some(session) = session_borrow.as_mut() else { return; };
                            let Some(chain) = session.project.chains.get_mut(chain_index) else { return; };
                            let Some(block) = chain.blocks.get_mut(block_index) else { return; };
                            block.enabled = !block.enabled;
                            let new_enabled = block.enabled;
                            let chain_id = chain.id.clone();
                            drop(session_borrow);
                            if let Some(draft) = win_draft.borrow_mut().as_mut() {
                                draft.enabled = new_enabled;
                            }
                            (chain_index, block_index, Some(chain_id))
                        };
                        let new_enabled = {
                            let session_borrow = project_session.borrow();
                            let Some(session) = session_borrow.as_ref() else { return; };
                            let Some(chain) = session.project.chains.get(chain_idx) else { return; };
                            let Some(block) = chain.blocks.get(block_idx) else { return; };
                            block.enabled
                        };
                        let Some(chain_id) = chain_id_opt else { return; };
                        let mut session_borrow = project_session.borrow_mut();
                        let Some(session) = session_borrow.as_mut() else { return; };
                        if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                            log::error!("[adapter-gui] block-window.toggle-enabled: {e}");
                            if let Some(w) = weak_main.upgrade() {
                                w.set_block_drawer_status_message(e.to_string().into());
                            }
                            return;
                        }
                        replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                        sync_project_dirty(&main, session, &saved_project_snapshot, &project_dirty, auto_save);
                        drop(session_borrow);
                        win.set_block_drawer_enabled(new_enabled);
                    });
                }
                // on_update_block_parameter_number
                {
                    let win_draft = win_draft.clone();
                    let win_param_items = win_param_items.clone();
                    let win_knob_overlays = win_knob_overlays.clone();
                    let win_eq_band_curves = win_eq_band_curves.clone();
                    let win_curve_editor_pts = win_curve_editor_pts.clone();
                    let win_timer = win_timer.clone();
                    let project_session = project_session.clone();
                    let project_chains = project_chains.clone();
                    let project_runtime = project_runtime.clone();
                    let saved_project_snapshot = saved_project_snapshot.clone();
                    let project_dirty = project_dirty.clone();
                    let input_chain_devices = input_chain_devices.clone();
                    let output_chain_devices = output_chain_devices.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_update_block_parameter_number(move |path, value| {
                        let Some(win) = weak_win.upgrade() else { return; };
                        set_block_parameter_number(&win_param_items, path.as_str(), value);
                        // Update overlay value so the knob indicator re-renders instantly
                        for i in 0..win_knob_overlays.row_count() {
                            if let Some(mut overlay) = win_knob_overlays.row_data(i) {
                                if overlay.path == path {
                                    overlay.value = value;
                                    win_knob_overlays.set_row_data(i, overlay);
                                    break;
                                }
                            }
                        }
                        // Recompute EQ curves and update curve editor point values in-place.
                        // Use set_row_data instead of set_vec to avoid recreating elements (which
                        // would reset the TouchArea pressed state and break drag interactions).
                        if let Some(draft) = win_draft.borrow().as_ref() {
                            let params = build_params_from_items(&win_param_items);
                            let (eq_total, eq_bands) = compute_eq_curves(&draft.effect_type, &draft.model_id, &params);
                            win_eq_band_curves.set_vec(eq_bands.into_iter().map(SharedString::from).collect::<Vec<_>>());
                            win.set_eq_total_curve(eq_total.into());
                            // Update matching curve editor point in-place by path
                            let path_str = path.as_str();
                            for idx in 0..win_curve_editor_pts.row_count() {
                                if let Some(mut pt) = win_curve_editor_pts.row_data(idx) {
                                    if pt.y_path.as_str() == path_str {
                                        pt.y_value = value;
                                        pt.y_label = if value >= 0.0 {
                                            format!("+{:.1}", value).into()
                                        } else {
                                            format!("{:.1}", value).into()
                                        };
                                        win_curve_editor_pts.set_row_data(idx, pt);
                                        break;
                                    } else if pt.has_x && pt.x_path.as_str() == path_str {
                                        pt.x_value = value;
                                        pt.x_label = if value >= 1000.0 {
                                            format!("{:.1}k", value / 1000.0).into()
                                        } else {
                                            format!("{}Hz", value as i32).into()
                                        };
                                        win_curve_editor_pts.set_row_data(idx, pt);
                                        break;
                                    } else if pt.has_width && pt.width_path.as_str() == path_str {
                                        pt.width_value = value;
                                        win_curve_editor_pts.set_row_data(idx, pt);
                                        break;
                                    }
                                }
                            }
                        }
                        if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                            schedule_block_editor_persist_for_block_win(
                                &win_timer, weak_win.clone(), weak_main.clone(),
                                win_draft.clone(), win_param_items.clone(),
                                project_session.clone(), project_chains.clone(), project_runtime.clone(),
                                saved_project_snapshot.clone(), project_dirty.clone(),
                                input_chain_devices.clone(), output_chain_devices.clone(),
                                "block-window.number",
                            auto_save,
                            );
                        }
                    });
                }
                // on_update_block_parameter_number_text
                {
                    let win_draft = win_draft.clone();
                    let win_param_items = win_param_items.clone();
                    let win_timer = win_timer.clone();
                    let project_session = project_session.clone();
                    let project_chains = project_chains.clone();
                    let project_runtime = project_runtime.clone();
                    let saved_project_snapshot = saved_project_snapshot.clone();
                    let project_dirty = project_dirty.clone();
                    let input_chain_devices = input_chain_devices.clone();
                    let output_chain_devices = output_chain_devices.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_update_block_parameter_number_text(move |path, value_text| {
                        let Some(_win) = weak_win.upgrade() else { return; };
                        let normalized = value_text.replace(',', ".");
                        let Ok(value) = normalized.parse::<f32>() else { return; };
                        set_block_parameter_number(&win_param_items, path.as_str(), value);
                        if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                            schedule_block_editor_persist_for_block_win(
                                &win_timer, weak_win.clone(), weak_main.clone(),
                                win_draft.clone(), win_param_items.clone(),
                                project_session.clone(), project_chains.clone(), project_runtime.clone(),
                                saved_project_snapshot.clone(), project_dirty.clone(),
                                input_chain_devices.clone(), output_chain_devices.clone(),
                                "block-window.number-text",
                            auto_save,
                            );
                        }
                    });
                }
                // on_update_block_parameter_bool
                {
                    let win_draft = win_draft.clone();
                    let win_param_items = win_param_items.clone();
                    let win_timer = win_timer.clone();
                    let project_session = project_session.clone();
                    let project_chains = project_chains.clone();
                    let project_runtime = project_runtime.clone();
                    let saved_project_snapshot = saved_project_snapshot.clone();
                    let project_dirty = project_dirty.clone();
                    let input_chain_devices = input_chain_devices.clone();
                    let output_chain_devices = output_chain_devices.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_update_block_parameter_bool(move |path, value| {
                        let Some(_win) = weak_win.upgrade() else { return; };
                        set_block_parameter_bool(&win_param_items, path.as_str(), value);
                        if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                            schedule_block_editor_persist_for_block_win(
                                &win_timer, weak_win.clone(), weak_main.clone(),
                                win_draft.clone(), win_param_items.clone(),
                                project_session.clone(), project_chains.clone(), project_runtime.clone(),
                                saved_project_snapshot.clone(), project_dirty.clone(),
                                input_chain_devices.clone(), output_chain_devices.clone(),
                                "block-window.bool",
                            auto_save,
                            );
                        }
                    });
                }
                // on_update_block_parameter_text
                {
                    let win_draft = win_draft.clone();
                    let win_param_items = win_param_items.clone();
                    let win_timer = win_timer.clone();
                    let project_session = project_session.clone();
                    let project_chains = project_chains.clone();
                    let project_runtime = project_runtime.clone();
                    let saved_project_snapshot = saved_project_snapshot.clone();
                    let project_dirty = project_dirty.clone();
                    let input_chain_devices = input_chain_devices.clone();
                    let output_chain_devices = output_chain_devices.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_update_block_parameter_text(move |path, value| {
                        let Some(_win) = weak_win.upgrade() else { return; };
                        set_block_parameter_text(&win_param_items, path.as_str(), value.as_str());
                        if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                            schedule_block_editor_persist_for_block_win(
                                &win_timer, weak_win.clone(), weak_main.clone(),
                                win_draft.clone(), win_param_items.clone(),
                                project_session.clone(), project_chains.clone(), project_runtime.clone(),
                                saved_project_snapshot.clone(), project_dirty.clone(),
                                input_chain_devices.clone(), output_chain_devices.clone(),
                                "block-window.text",
                            auto_save,
                            );
                        }
                    });
                }
                // on_select_block_parameter_option
                {
                    let win_draft = win_draft.clone();
                    let win_param_items = win_param_items.clone();
                    let win_timer = win_timer.clone();
                    let project_session = project_session.clone();
                    let project_chains = project_chains.clone();
                    let project_runtime = project_runtime.clone();
                    let saved_project_snapshot = saved_project_snapshot.clone();
                    let project_dirty = project_dirty.clone();
                    let input_chain_devices = input_chain_devices.clone();
                    let output_chain_devices = output_chain_devices.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_select_block_parameter_option(move |path, index| {
                        let Some(_win) = weak_win.upgrade() else { return; };
                        set_block_parameter_option(&win_param_items, path.as_str(), index);
                        if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                            schedule_block_editor_persist_for_block_win(
                                &win_timer, weak_win.clone(), weak_main.clone(),
                                win_draft.clone(), win_param_items.clone(),
                                project_session.clone(), project_chains.clone(), project_runtime.clone(),
                                saved_project_snapshot.clone(), project_dirty.clone(),
                                input_chain_devices.clone(), output_chain_devices.clone(),
                                "block-window.option",
                            auto_save,
                            );
                        }
                    });
                }
                // on_pick_block_parameter_file
                {
                    let win_draft = win_draft.clone();
                    let win_param_items = win_param_items.clone();
                    let win_timer = win_timer.clone();
                    let project_session = project_session.clone();
                    let project_chains = project_chains.clone();
                    let project_runtime = project_runtime.clone();
                    let saved_project_snapshot = saved_project_snapshot.clone();
                    let project_dirty = project_dirty.clone();
                    let input_chain_devices = input_chain_devices.clone();
                    let output_chain_devices = output_chain_devices.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_pick_block_parameter_file(move |path| {
                        let Some(_win) = weak_win.upgrade() else { return; };
                        let extensions = block_parameter_extensions(&win_param_items, path.as_str());
                        let mut dialog = FileDialog::new();
                        if !extensions.is_empty() {
                            let refs: Vec<&str> = extensions.iter().map(|v| v.as_str()).collect();
                            dialog = dialog.add_filter("Arquivos suportados", &refs);
                        }
                        let Some(file) = dialog.pick_file() else { return; };
                        set_block_parameter_text(&win_param_items, path.as_str(), file.to_string_lossy().as_ref());
                        if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                            schedule_block_editor_persist_for_block_win(
                                &win_timer, weak_win.clone(), weak_main.clone(),
                                win_draft.clone(), win_param_items.clone(),
                                project_session.clone(), project_chains.clone(), project_runtime.clone(),
                                saved_project_snapshot.clone(), project_dirty.clone(),
                                input_chain_devices.clone(), output_chain_devices.clone(),
                                "block-window.file",
                            auto_save,
                            );
                        }
                    });
                }
                // on_open_vst3_editor (opens native plugin GUI window)
                {
                    let vst3_handles = vst3_editor_handles.clone();
                    let vst3_sr = vst3_sample_rate;
                    win.on_open_vst3_editor(move |model_id| {
                        match project::vst3_editor::open_vst3_editor(model_id.as_str(), vst3_sr) {
                            Ok(handle) => { vst3_handles.borrow_mut().push(handle); }
                            Err(e) => { log::error!("VST3 editor: failed '{}': {}", model_id, e); }
                        }
                    });
                }
                // on_save_block_drawer (edit mode - saves and closes)
                {
                    let win_draft = win_draft.clone();
                    let win_param_items = win_param_items.clone();
                    let win_timer = win_timer.clone();
                    let project_session = project_session.clone();
                    let project_chains = project_chains.clone();
                    let project_runtime = project_runtime.clone();
                    let saved_project_snapshot = saved_project_snapshot.clone();
                    let project_dirty = project_dirty.clone();
                    let input_chain_devices = input_chain_devices.clone();
                    let output_chain_devices = output_chain_devices.clone();
                    let selected_block = selected_block.clone();
                    let open_block_windows = open_block_windows.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_save_block_drawer(move || {
                        let Some(win) = weak_win.upgrade() else { return; };
                        let Some(main) = weak_main.upgrade() else { return; };
                        win_timer.stop();
                        let Some(draft) = win_draft.borrow().clone() else { return; };
                        if let Err(e) = persist_block_editor_draft(
                            &main, &draft, &win_param_items,
                            &project_session, &project_chains, &project_runtime,
                            &saved_project_snapshot, &project_dirty,
                            &*input_chain_devices.borrow(), &*output_chain_devices.borrow(), true,
                            auto_save,
                        ) {
                            log::error!("[adapter-gui] block-window.save: {e}");
                            main.set_block_drawer_status_message(e.to_string().into());
                            return;
                        }
                        *selected_block.borrow_mut() = None;
                        set_selected_block(&main, None, None);
                        open_block_windows.borrow_mut().retain(|bw| {
                            bw.chain_index != draft.chain_index
                                || bw.block_index != draft.block_index.unwrap_or(usize::MAX)
                        });
                        let _ = win.hide();
                    });
                }
                // on_delete_block_drawer
                {
                    let win_draft = win_draft.clone();
                    let win_timer = win_timer.clone();
                    let project_session = project_session.clone();
                    let project_chains = project_chains.clone();
                    let project_runtime = project_runtime.clone();
                    let saved_project_snapshot = saved_project_snapshot.clone();
                    let project_dirty = project_dirty.clone();
                    let input_chain_devices = input_chain_devices.clone();
                    let output_chain_devices = output_chain_devices.clone();
                    let selected_block = selected_block.clone();
                    let open_block_windows = open_block_windows.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_delete_block_drawer(move || {
                        let Some(win) = weak_win.upgrade() else { return; };
                        let Some(main) = weak_main.upgrade() else { return; };
                        win_timer.stop();
                        let Some(draft) = win_draft.borrow().clone() else { return; };
                        let Some(block_index) = draft.block_index else { return; };
                        let confirmed = rfd::MessageDialog::new()
                            .set_title("Excluir bloco")
                            .set_description(format!("Excluir o bloco \"{}\"?", draft.model_id))
                            .set_buttons(rfd::MessageButtons::YesNo)
                            .set_level(rfd::MessageLevel::Warning)
                            .show();
                        if !matches!(confirmed, rfd::MessageDialogResult::Yes) {
                            return;
                        }
                        let mut session_borrow = project_session.borrow_mut();
                        let Some(session) = session_borrow.as_mut() else { return; };
                        let Some(chain) = session.project.chains.get_mut(draft.chain_index) else { return; };
                        if block_index >= chain.blocks.len() { return; }
                        let chain_id = chain.id.clone();
                        chain.blocks.remove(block_index);
                        if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                            log::error!("[adapter-gui] block-window.delete: {e}");
                            if let Some(w) = weak_main.upgrade() {
                                w.set_block_drawer_status_message(e.to_string().into());
                            }
                            return;
                        }
                        replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                        sync_project_dirty(&main, session, &saved_project_snapshot, &project_dirty, auto_save);
                        drop(session_borrow);
                        *selected_block.borrow_mut() = None;
                        set_selected_block(&main, None, None);
                        open_block_windows.borrow_mut().retain(|bw| {
                            bw.chain_index != draft.chain_index || bw.block_index != block_index
                        });
                        let _ = win.hide();
                    });
                }
                // on_show_plugin_info
                {
                    let weak_window = window.as_weak();
                    let plugin_info_window = plugin_info_window.clone();
                    win.on_show_plugin_info(move |effect_type, model_id| {
                        let Some(window) = weak_window.upgrade() else {
                            return;
                        };
                        let effect_type = effect_type.to_string();
                        let model_id = model_id.to_string();

                        let display_name = model_display_name(&effect_type, &model_id);
                        let brand = model_brand(&effect_type, &model_id);
                        let type_label = model_type_label(&effect_type, &model_id);

                        let lang = system_language();
                        let meta = plugin_info::plugin_metadata(&lang, &model_id);
            
                        let (screenshot_img, has_screenshot) = load_screenshot_image(&effect_type, &model_id);

                        let info_win = match PluginInfoWindow::new() {
                            Ok(w) => w,
                            Err(e) => {
                                log::error!("Failed to create PluginInfoWindow: {}", e);
                                return;
                            }
                        };

                        info_win.set_plugin_name(display_name.into());
                        info_win.set_brand(brand.into());
                        info_win.set_type_label(type_label.into());
                        info_win.set_description(meta.description.into());
                        info_win.set_license(meta.license.into());
                        info_win.set_has_homepage(!meta.homepage.is_empty());
                        info_win.set_homepage(meta.homepage.clone().into());
                        info_win.set_screenshot(screenshot_img);
                        info_win.set_has_screenshot(has_screenshot);

                        {
                            let homepage = meta.homepage.clone();
                            info_win.on_open_homepage(move || {
                                plugin_info::open_homepage(&homepage);
                            });
                        }

                        {
                            let win_weak = info_win.as_weak();
                            info_win.on_close_window(move || {
                                if let Some(w) = win_weak.upgrade() {
                                    let _ = w.window().hide();
                                }
                            });
                        }

                        *plugin_info_window.borrow_mut() = Some(info_win);
                        if let Some(w) = plugin_info_window.borrow().as_ref() {
                            show_child_window(window.window(), w.window());
                        }
                    });
                }
                // on_close_block_drawer (close without saving)
                {
                    let win_draft = win_draft.clone();
                    let open_block_windows = open_block_windows.clone();
                    let selected_block = selected_block.clone();
                    let weak_main = weak_main_window.clone();
                    let weak_win = win.as_weak();
                    win.on_close_block_drawer(move || {
                        let Some(win) = weak_win.upgrade() else { return; };
                        let Some(main) = weak_main.upgrade() else { return; };
                        let draft_borrow = win_draft.borrow();
                        if let Some(draft) = draft_borrow.as_ref() {
                            open_block_windows.borrow_mut().retain(|bw| {
                                bw.chain_index != draft.chain_index || Some(bw.block_index) != draft.block_index
                            });
                        }
                        drop(draft_borrow);
                        *selected_block.borrow_mut() = None;
                        set_selected_block(&main, None, None);
                        let _ = win.hide();
                    });
                }
                // Clean up stream timer when block editor is closed via the window X button.
                {
                    let open_block_windows_close = open_block_windows.clone();
                    win.window().on_close_requested(move || {
                        open_block_windows_close.borrow_mut().retain(|bw| {
                            bw.chain_index != ci || bw.block_index != bi
                        });
                        slint::CloseRequestResponse::HideWindow
                    });
                }
                show_child_window(window.window(), win.window());
                open_block_windows.borrow_mut().push(BlockWindow { chain_index: ci, block_index: bi, window: win, stream_timer: block_stream_timer });
            }
        });
    }
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
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_type_options = block_type_options.clone();
        let block_model_options = block_model_options.clone();
        let filtered_block_model_options = filtered_block_model_options.clone();
        let block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let multi_slider_points = multi_slider_points.clone();
        let curve_editor_points = curve_editor_points.clone();
        let eq_band_curves = eq_band_curves.clone();
        let project_session = project_session.clone();
        window.on_start_block_insert(move |chain_index, before_index| {
            log::debug!("on_start_block_insert: chain_index={}, before_index={}", chain_index, before_index);
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let instrument = project_session.borrow().as_ref()
                .and_then(|s| {
                    let chain = s.project.chains.get(chain_index as usize)?;
                    log::info!("=== START_BLOCK_INSERT: chain_index={}, chain.instrument='{}', chain.description={:?} ===",
                        chain_index, chain.instrument, chain.description);
                    Some(chain.instrument.clone())
                })
                .unwrap_or_else(|| {
                    log::warn!("=== START_BLOCK_INSERT: no chain at index {}, defaulting to electric_guitar ===", chain_index);
                    block_core::DEFAULT_INSTRUMENT.to_string()
                });
            // Map UI before_index to real chain.blocks index (UI excludes hidden I/O blocks)
            let real_before_index = {
                let session_borrow = project_session.borrow();
                session_borrow.as_ref()
                    .and_then(|s| s.project.chains.get(chain_index as usize))
                    .map(|chain| ui_index_to_real_block_index(chain, before_index as usize))
                    .unwrap_or(before_index as usize)
            };
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = Some(BlockEditorDraft {
                chain_index: chain_index as usize,
                block_index: None,
                before_index: real_before_index,
                instrument: instrument.clone(),
                effect_type: String::new(),
                model_id: String::new(),
                enabled: true,
                is_select: false,
            });
            block_type_options.set_vec(block_type_picker_items(&instrument));
            block_model_options.set_vec(Vec::new());
            filtered_block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
            multi_slider_points.set_vec(Vec::new());
            curve_editor_points.set_vec(Vec::new());
            eq_band_curves.set_vec(Vec::new());
            window.set_eq_total_curve("".into());
            set_selected_block(&window, None, None);
            window.set_block_drawer_edit_mode(false);
            window.set_block_drawer_selected_type_index(-1);
            window.set_block_drawer_selected_model_index(-1);
            window.set_block_drawer_status_message("".into());
            window.set_show_block_drawer(false);
            window.set_show_block_type_picker(true);
        });
    }
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_model_options = block_model_options.clone();
        let filtered_block_model_options = filtered_block_model_options.clone();
        let block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let multi_slider_points = multi_slider_points.clone();
        let curve_editor_points = curve_editor_points.clone();
        let eq_band_curves = eq_band_curves.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let chain_draft_for_type = chain_draft.clone();
        let io_block_insert_draft_for_type = io_block_insert_draft.clone();
        let weak_input_window_for_type = chain_input_window.as_weak();
        let weak_output_window_for_type = chain_output_window.as_weak();
        let project_session_for_type = project_session.clone();
        let project_runtime_for_type = project_runtime.clone();
        let project_chains_for_type = project_chains.clone();
        let saved_project_snapshot_for_type = saved_project_snapshot.clone();
        let project_dirty_for_type = project_dirty.clone();
        let input_chain_devices_for_type = input_chain_devices.clone();
        let output_chain_devices_for_type = output_chain_devices.clone();
        let chain_input_device_options_for_type = chain_input_device_options.clone();
        let chain_output_device_options_for_type = chain_output_device_options.clone();
        let chain_input_channels_for_type = chain_input_channels.clone();
        let chain_output_channels_for_type = chain_output_channels.clone();
        let weak_insert_window_for_type = chain_insert_window.as_weak();
        let insert_draft_for_type = insert_draft.clone();
        let insert_send_channels_for_type = insert_send_channels.clone();
        let insert_return_channels_for_type = insert_return_channels.clone();
        window.on_choose_block_type(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let instrument = block_editor_draft.borrow().as_ref()
                .map(|d| d.instrument.clone())
                .unwrap_or_else(|| block_core::DEFAULT_INSTRUMENT.to_string());
            let block_types = block_type_picker_items(&instrument);
            let Some(block_type) = block_types.get(index as usize) else {
                return;
            };
            log::debug!("on_choose_block_type: index={}, type='{}', instrument='{}'", index, block_type.effect_type, instrument);

            // Handle I/O and Insert block types: open the dedicated window instead of the block editor
            let effect_type_str = block_type.effect_type.as_str();
            if effect_type_str == "insert" {
                // Insert block: create directly with empty endpoints
                let (chain_index, before_index) = {
                    let draft_borrow = block_editor_draft.borrow();
                    let Some(draft) = draft_borrow.as_ref() else { return; };
                    (draft.chain_index, draft.before_index)
                };
                let session_borrow = project_session_for_type.borrow();
                let Some(session) = session_borrow.as_ref() else { return; };
                let Some(chain) = session.project.chains.get(chain_index) else { return; };
                let block_id = domain::ids::BlockId(format!("{}:insert:{}", chain.id.0, before_index));
                drop(session_borrow);
                let insert_block = project::block::AudioBlock {
                    id: block_id,
                    enabled: true,
                    kind: AudioBlockKind::Insert(project::block::InsertBlock {
                        model: "standard".to_string(),
                        send: project::block::InsertEndpoint {
                            device_id: domain::ids::DeviceId(String::new()),
                            mode: ChainInputMode::Mono,
                            channels: Vec::new(),
                        },
                        return_: project::block::InsertEndpoint {
                            device_id: domain::ids::DeviceId(String::new()),
                            mode: ChainInputMode::Mono,
                            channels: Vec::new(),
                        },
                    }),
                };
                let mut session_borrow = project_session_for_type.borrow_mut();
                let Some(session) = session_borrow.as_mut() else { return; };
                let Some(chain) = session.project.chains.get_mut(chain_index) else { return; };
                chain.blocks.insert(before_index, insert_block);
                let chain_id = chain.id.clone();
                if let Err(e) = sync_live_chain_runtime(&project_runtime_for_type, session, &chain_id) {
                    log::error!("insert block create error: {e}");
                }
                replace_project_chains(&project_chains_for_type, &session.project, &*input_chain_devices_for_type.borrow(), &*output_chain_devices_for_type.borrow());
                sync_project_dirty(&window, session, &saved_project_snapshot_for_type, &project_dirty_for_type, auto_save);
                window.set_show_block_type_picker(false);
                // Open the insert window to configure the newly created block
                drop(session_borrow);
                let draft = InsertDraft {
                    chain_index,
                    block_index: before_index,
                    send_device_id: None,
                    send_channels: Vec::new(),
                    send_mode: ChainInputMode::Mono,
                    return_device_id: None,
                    return_channels: Vec::new(),
                    return_mode: ChainInputMode::Mono,
                };
                if let Some(iw) = weak_insert_window_for_type.upgrade() {
                    refresh_input_devices(&chain_input_device_options_for_type);
                    refresh_output_devices(&chain_output_device_options_for_type);
                    replace_channel_options(&insert_send_channels_for_type, Vec::new());
                    replace_channel_options(&insert_return_channels_for_type, Vec::new());
                    iw.set_selected_send_device_index(-1);
                    iw.set_selected_return_device_index(-1);
                    iw.set_selected_send_mode_index(0);
                    iw.set_selected_return_mode_index(0);
                    iw.set_show_block_controls(true);
                    iw.set_block_enabled(true);
                    iw.set_status_message("".into());
                    *insert_draft_for_type.borrow_mut() = Some(draft);
                    show_child_window(window.window(), iw.window());
                }
                return;
            }
            if effect_type_str == "input" || effect_type_str == "output" {
                let (chain_index, before_index) = {
                    let draft_borrow = block_editor_draft.borrow();
                    let Some(draft) = draft_borrow.as_ref() else { return; };
                    (draft.chain_index, draft.before_index)
                };
                // Store the I/O insert draft
                *io_block_insert_draft_for_type.borrow_mut() = Some(IoBlockInsertDraft {
                    chain_index,
                    before_index,
                    kind: effect_type_str.to_string(),
                });
                window.set_show_block_type_picker(false);

                if effect_type_str == "input" {
                    // Set up a temporary chain draft for the input window callbacks
                    let input_group = InputGroupDraft {
                        device_id: None,
                        channels: Vec::new(),
                        mode: ChainInputMode::Mono,
                    };
                    *chain_draft_for_type.borrow_mut() = Some(ChainDraft {
                        editing_index: Some(chain_index),
                        name: String::new(),
                        instrument: instrument.clone(),
                        inputs: vec![input_group.clone()],
                        outputs: Vec::new(),
                        editing_input_index: Some(0),
                        editing_output_index: None,
        editing_io_block_index: None,
                        adding_new_input: false,
                        adding_new_output: false,
                    });
                    if let Some(input_window) = weak_input_window_for_type.upgrade() {
                        let fresh_input = refresh_input_devices(&chain_input_device_options_for_type);
                        let draft_borrow = chain_draft_for_type.borrow();
                        let draft = draft_borrow.as_ref().unwrap();
                        if let Some(session) = project_session_for_type.borrow().as_ref() {
                            apply_chain_input_window_state(
                                &input_window,
                                &input_group,
                                draft,
                                &session.project,
                                &fresh_input,
                                &chain_input_channels_for_type,
                            );
                        }
                        show_child_window(window.window(), input_window.window());
                    }
                } else {
                    // Set up a temporary chain draft for the output window callbacks
                    let output_group = OutputGroupDraft {
                        device_id: None,
                        channels: Vec::new(),
                        mode: ChainOutputMode::Stereo,
                    };
                    *chain_draft_for_type.borrow_mut() = Some(ChainDraft {
                        editing_index: Some(chain_index),
                        name: String::new(),
                        instrument: instrument.clone(),
                        inputs: Vec::new(),
                        outputs: vec![output_group.clone()],
        editing_io_block_index: None,
                        editing_input_index: None,
                        editing_output_index: Some(0),
                        adding_new_input: false,
                        adding_new_output: false,
                    });
                    if let Some(output_window) = weak_output_window_for_type.upgrade() {
                        let fresh_output = refresh_output_devices(&chain_output_device_options_for_type);
                        apply_chain_output_window_state(
                            &output_window,
                            &output_group,
                            &fresh_output,
                            &chain_output_channels_for_type,
                        );
                        show_child_window(window.window(), output_window.window());
                    }
                }
                return;
            }

            let models = block_model_picker_items(block_type.effect_type.as_str(), &instrument);
            let Some(model) = models.first() else {
                return;
            };
            if let Some(draft) = block_editor_draft.borrow_mut().as_mut() {
                draft.effect_type = model.effect_type.to_string();
                draft.model_id = model.model_id.to_string();
            }
            let items = block_model_picker_items(block_type.effect_type.as_str(), &instrument);
            block_model_option_labels.set_vec(block_model_picker_labels(&items));
            block_model_options.set_vec(items.clone());
            filtered_block_model_options.set_vec(items);
            let new_params = block_parameter_items_for_model(
                &model.effect_type,
                &model.model_id,
                &ParameterSet::default(),
            );
            let overlays = build_knob_overlays(project::catalog::model_knob_layout(&model.effect_type, &model.model_id), &new_params);
            block_parameter_items.set_vec(new_params);
            multi_slider_points.set_vec(build_multi_slider_points(&model.effect_type, &model.model_id, &ParameterSet::default()));
            curve_editor_points.set_vec(build_curve_editor_points(&model.effect_type, &model.model_id, &ParameterSet::default()));
            let (eq_total, eq_bands) = compute_eq_curves(&model.effect_type, &model.model_id, &ParameterSet::default());
            eq_band_curves.set_vec(eq_bands.into_iter().map(SharedString::from).collect::<Vec<_>>());
            window.set_eq_total_curve(eq_total.into());
            let drawer_state =
                block_drawer_state(None, &model.effect_type, Some(&model.model_id));
            window.set_block_drawer_title(drawer_state.title.into());
            window.set_block_drawer_confirm_label(drawer_state.confirm_label.into());
            window.set_block_drawer_edit_mode(false);
            window.set_block_drawer_selected_type_index(index);
            window.set_block_drawer_selected_model_index(0);
            window.set_block_drawer_status_message("".into());
            window.set_show_block_type_picker(false);
            if use_inline_block_editor(&window) {
                window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
                window.set_show_block_drawer(true);
            } else {
                window.set_show_block_drawer(false);
                if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                    block_editor_window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
                    sync_block_editor_window(&window, &block_editor_window);
                    show_child_window(window.window(), block_editor_window.window());
                }
            }
        });
    }
    // --- Block model search callbacks (extracted to block_model_search_wiring) ---
    crate::block_model_search_wiring::wire(
        &window,
        &block_editor_window,
        block_model_options.clone(),
        filtered_block_model_options.clone(),
    );
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let multi_slider_points = multi_slider_points.clone();
        let curve_editor_points = curve_editor_points.clone();
        let eq_band_curves = eq_band_curves.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_choose_block_model(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = block_editor_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let models = block_model_picker_items(&draft.effect_type, &draft.instrument);
            let Some(model) = models.get(index as usize) else {
                return;
            };
            log::debug!("on_choose_block_model: index={}, model_id='{}', effect_type='{}'", index, model.model_id, model.effect_type);
            draft.model_id = model.model_id.to_string();
            draft.effect_type = model.effect_type.to_string();
            let new_params = block_parameter_items_for_model(
                &model.effect_type,
                &model.model_id,
                &ParameterSet::default(),
            );
            let overlays = build_knob_overlays(project::catalog::model_knob_layout(&model.effect_type, &model.model_id), &new_params);
            block_parameter_items.set_vec(new_params);
            multi_slider_points.set_vec(build_multi_slider_points(&model.effect_type, &model.model_id, &ParameterSet::default()));
            curve_editor_points.set_vec(build_curve_editor_points(&model.effect_type, &model.model_id, &ParameterSet::default()));
            let (eq_total, eq_bands) = compute_eq_curves(&model.effect_type, &model.model_id, &ParameterSet::default());
            eq_band_curves.set_vec(eq_bands.into_iter().map(SharedString::from).collect::<Vec<_>>());
            window.set_eq_total_curve(eq_total.into());
            window.set_block_drawer_selected_model_index(index);
            window.set_block_drawer_status_message("".into());
            if use_inline_block_editor(&window) {
                window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
            } else if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
                sync_block_editor_window(&window, &block_editor_window);
            }
            if draft.block_index.is_some() {
                schedule_block_editor_persist(
                    &block_editor_persist_timer,
                    weak_window.clone(),
                    block_editor_draft.clone(),
                    block_parameter_items.clone(),
                    project_session.clone(),
                    project_chains.clone(),
                    project_runtime.clone(),
                    saved_project_snapshot.clone(),
                    project_dirty.clone(),
                    input_chain_devices.clone(),
                    output_chain_devices.clone(),
                    "block-drawer.choose-model",
                    auto_save,
                );
            }
        });
    }
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
    // Fullscreen inline I/O endpoint editor callbacks — delegate based on flow
    {
        let chain_editor_window = chain_editor_window.clone();
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        window.on_chain_io_select_device(move |index| {
            let from_groups = weak_window.upgrade().map_or(false, |w| w.get_show_chain_io_groups());
            if from_groups {
                if inline_flag.get() {
                    if let Some(iw) = weak_input_window.upgrade() { iw.invoke_select_device(index); }
                } else {
                    if let Some(ow) = weak_output_window.upgrade() { ow.invoke_select_device(index); }
                }
            } else {
                if let Some(cew) = chain_editor_window.borrow().as_ref() {
                    if cew.get_show_input_editor() {
                        cew.invoke_input_select_device(index);
                    } else if cew.get_show_output_editor() {
                        cew.invoke_output_select_device(index);
                    }
                }
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        let weak_window = window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        window.on_chain_io_toggle_channel(move |index, selected| {
            let Some(w) = weak_window.upgrade() else { return; };
            let from_groups = w.get_show_chain_io_groups();
            if from_groups {
                // Delegate to AppWindow's toggle handler which updates the
                // shared chain_input_channels / chain_output_channels VecModel.
                // Since on_chain_io_groups_edit already set chain-io-channels
                // to point at the same shared VecModel, changes are reflected
                // automatically — no sync needed.
                if inline_flag.get() {
                    w.invoke_toggle_chain_input_channel(index, selected);
                } else {
                    w.invoke_toggle_chain_output_channel(index, selected);
                }
            } else {
                if let Some(cew) = chain_editor_window.borrow().as_ref() {
                    if cew.get_show_input_editor() {
                        cew.invoke_input_toggle_channel(index, selected);
                    } else if cew.get_show_output_editor() {
                        cew.invoke_output_toggle_channel(index, selected);
                    }
                }
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        window.on_chain_io_select_mode(move |index| {
            let from_groups = weak_window.upgrade().map_or(false, |w| w.get_show_chain_io_groups());
            if from_groups {
                if inline_flag.get() {
                    if let Some(iw) = weak_input_window.upgrade() { iw.invoke_select_input_mode(index); }
                } else {
                    if let Some(ow) = weak_output_window.upgrade() { ow.invoke_select_output_mode(index); }
                }
            } else {
                if let Some(cew) = chain_editor_window.borrow().as_ref() {
                    if cew.get_show_input_editor() {
                        cew.invoke_input_select_mode(index);
                    } else if cew.get_show_output_editor() {
                        cew.invoke_output_select_mode(index);
                    }
                }
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        window.on_chain_io_save(move || {
            let Some(w) = weak_window.upgrade() else { return; };
            if w.get_show_chain_io_groups() {
                // Came from groups flow — delegate to ChainInputWindow/ChainOutputWindow
                if inline_flag.get() {
                    if let Some(iw) = weak_input_window.upgrade() {
                        iw.invoke_save();
                    }
                    // Sync groups back after save
                    if let Some(gw) = weak_input_groups.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                } else {
                    if let Some(ow) = weak_output_window.upgrade() {
                        ow.invoke_save();
                    }
                    if let Some(gw) = weak_output_groups.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                }
            } else {
                // Came from chain editor flow
                if let Some(cew) = chain_editor_window.borrow().as_ref() {
                    if cew.get_show_input_editor() {
                        cew.invoke_input_save();
                    } else if cew.get_show_output_editor() {
                        cew.invoke_output_save();
                    }
                }
            }
            w.set_show_chain_io_editor(false);
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        window.on_chain_io_cancel(move || {
            let Some(w) = weak_window.upgrade() else { return; };
            if w.get_show_chain_io_groups() {
                // Came from groups flow — delegate cancel
                if inline_flag.get() {
                    if let Some(iw) = weak_input_window.upgrade() {
                        iw.invoke_cancel();
                    }
                } else {
                    if let Some(ow) = weak_output_window.upgrade() {
                        ow.invoke_cancel();
                    }
                }
            } else {
                // Came from chain editor flow
                if let Some(cew) = chain_editor_window.borrow().as_ref() {
                    if cew.get_show_input_editor() {
                        cew.invoke_input_cancel();
                    } else if cew.get_show_output_editor() {
                        cew.invoke_output_cancel();
                    }
                }
            }
            w.set_show_chain_io_editor(false);
        });
    }
    // Fullscreen inline I/O groups callbacks — delegate to ChainInputGroupsWindow / ChainOutputGroupsWindow
    {
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_channels = chain_input_channels.clone();
        let chain_output_channels = chain_output_channels.clone();
        window.on_chain_io_groups_edit(move |group_index| {
            let Some(window) = weak_window.upgrade() else { return; };
            if inline_flag.get() {
                // In fullscreen, we set up draft state directly instead of
                // calling invoke_edit_group() which would open a child window.
                let fresh_input = refresh_input_devices(&chain_input_device_options);
                let mut draft_borrow = chain_draft.borrow_mut();
                if let Some(draft) = draft_borrow.as_mut() {
                    let gi = group_index as usize;
                    draft.editing_input_index = Some(gi);
                    if let Some(input_group) = draft.inputs.get(gi) {
                        let session_borrow = project_session.borrow();
                        if let Some(session) = session_borrow.as_ref() {
                            let dev_idx = selected_device_index(
                                &fresh_input,
                                input_group.device_id.as_deref(),
                            );
                            let mode_idx = input_mode_to_index(input_group.mode);
                            let channel_items = build_input_channel_items(input_group, draft, &session.project, &fresh_input);
                            let labels: Vec<slint::SharedString> = vec!["Mono".into(), "Stereo".into(), "Dual Mono".into()];
                            let device_strings: Vec<slint::SharedString> = fresh_input.iter().map(|d| slint::SharedString::from(d.name.as_str())).collect();
                            // Sync shared VecModel so toggle_chain_input_channel works
                            log::info!(
                                "[groups_edit INPUT] gi={} dev_idx={} device_id={:?} fresh_devices={} channel_items={} mode_idx={}",
                                gi, dev_idx, input_group.device_id, fresh_input.len(), channel_items.len(), mode_idx
                            );
                            for (ci, ch) in channel_items.iter().enumerate() {
                                log::info!("[groups_edit INPUT]   ch[{}] label='{}' selected={} available={}", ci, ch.label, ch.selected, ch.available);
                            }
                            replace_channel_options(&chain_input_channels, channel_items.clone());
                            window.set_chain_io_editor_title("Entrada".into());
                            window.set_chain_io_device_options(ModelRc::from(Rc::new(VecModel::from(device_strings.clone()))));
                            log::info!("[groups_edit INPUT] device_strings={:?}", device_strings);
                            window.set_chain_io_selected_device_index(dev_idx);
                            window.set_chain_io_channels(ModelRc::from(chain_input_channels.clone()));
                            window.set_chain_io_editor_status("".into());
                            window.set_chain_io_show_mode_selector(true);
                            window.set_chain_io_mode_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
                            window.set_chain_io_selected_mode_index(mode_idx);
                            window.set_show_chain_io_editor(true);
                        }
                    }
                }
            } else {
                // In fullscreen, we set up draft state directly instead of
                // calling invoke_edit_group() which would open a child window.
                let fresh_output = refresh_output_devices(&chain_output_device_options);
                let mut draft_borrow = chain_draft.borrow_mut();
                if let Some(draft) = draft_borrow.as_mut() {
                    let gi = group_index as usize;
                    draft.editing_output_index = Some(gi);
                    if let Some(output_group) = draft.outputs.get(gi) {
                        let dev_idx = selected_device_index(
                            &fresh_output,
                            output_group.device_id.as_deref(),
                        );
                        let mode_idx = output_mode_to_index(output_group.mode);
                        let channel_items = build_output_channel_items(output_group, &fresh_output);
                        let labels: Vec<slint::SharedString> = vec!["Mono".into(), "Stereo".into()];
                        let device_strings: Vec<slint::SharedString> = fresh_output.iter().map(|d| slint::SharedString::from(d.name.as_str())).collect();
                        // Sync shared VecModel so toggle_chain_output_channel works
                        log::info!(
                            "[groups_edit OUTPUT] gi={} dev_idx={} device_id={:?} fresh_devices={} channel_items={} mode_idx={}",
                            gi, dev_idx, output_group.device_id, fresh_output.len(), channel_items.len(), mode_idx
                        );
                        for (ci, ch) in channel_items.iter().enumerate() {
                            log::info!("[groups_edit OUTPUT]   ch[{}] label='{}' selected={} available={}", ci, ch.label, ch.selected, ch.available);
                        }
                        replace_channel_options(&chain_output_channels, channel_items.clone());
                        window.set_chain_io_editor_title("Saída".into());
                        window.set_chain_io_device_options(ModelRc::from(Rc::new(VecModel::from(device_strings.clone()))));
                        log::info!("[groups_edit OUTPUT] device_strings={:?}", device_strings);
                        window.set_chain_io_selected_device_index(dev_idx);
                        window.set_chain_io_channels(ModelRc::from(chain_output_channels.clone()));
                        window.set_chain_io_editor_status("".into());
                        window.set_chain_io_show_mode_selector(true);
                        window.set_chain_io_mode_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
                        window.set_chain_io_selected_mode_index(mode_idx);
                        window.set_show_chain_io_editor(true);
                    }
                }
            }
        });
    }
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        window.on_chain_io_groups_remove(move |group_index| {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_remove_group(group_index);
                    // Sync updated groups back to AppWindow
                    if let Some(w) = weak_window.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_remove_group(group_index);
                    if let Some(w) = weak_window.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                }
            }
        });
    }
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        window.on_chain_io_groups_add(move || {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_add_group();
                    if let Some(w) = weak_window.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_add_group();
                    if let Some(w) = weak_window.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                }
            }
        });
    }
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        window.on_chain_io_groups_save(move || {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_save();
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_save();
                }
            }
            if let Some(w) = weak_window.upgrade() {
                w.set_show_chain_io_groups(false);
            }
        });
    }
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        window.on_chain_io_groups_cancel(move || {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_cancel();
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_cancel();
                }
            }
            if let Some(w) = weak_window.upgrade() {
                w.set_show_chain_io_groups(false);
            }
        });
    }
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        window.on_chain_io_groups_toggle_enabled(move || {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_toggle_enabled();
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_toggle_enabled();
                }
            }
        });
    }
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        window.on_chain_io_groups_delete_block(move || {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_delete_block();
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_delete_block();
                }
            }
            if let Some(w) = weak_window.upgrade() {
                w.set_show_chain_io_groups(false);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_save_chain(move || {
            log::info!("on_save_chain triggered");
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let draft = match chain_draft.borrow().clone() {
                Some(draft) => draft,
                None => {
                    set_status_error(&window, &toast_timer, "Nenhuma chain em edição.");
                    return;
                }
            };
            if draft.inputs.is_empty() {
                set_status_warning(&window, &toast_timer, "Adicione pelo menos uma entrada.");
                return;
            }
            if draft.outputs.is_empty() {
                set_status_warning(&window, &toast_timer, "Adicione pelo menos uma saída.");
                return;
            }
            for (i, input) in draft.inputs.iter().enumerate() {
                if input.device_id.is_none() {
                    set_status_warning(&window, &toast_timer, &format!("Entrada {}: selecione o dispositivo.", i + 1));
                    return;
                }
                if input.channels.is_empty() {
                    set_status_warning(&window, &toast_timer, &format!("Entrada {}: selecione pelo menos um canal.", i + 1));
                    return;
                }
            }
            for (i, output) in draft.outputs.iter().enumerate() {
                if output.device_id.is_none() {
                    set_status_warning(&window, &toast_timer, &format!("Saída {}: selecione o dispositivo.", i + 1));
                    return;
                }
                if output.channels.is_empty() {
                    set_status_warning(&window, &toast_timer, &format!("Saída {}: selecione pelo menos um canal.", i + 1));
                    return;
                }
            }
            let editing_index = draft.editing_index;
            log::debug!("[save_chain] editing_index={:?}, draft.instrument='{}'", editing_index, draft.instrument);
            let existing_chain =
                editing_index.and_then(|index| session.project.chains.get(index).cloned());
            let chain = chain_from_draft(&draft, existing_chain.as_ref());
            if let Err(msg) = chain.validate_channel_conflicts() {
                set_status_warning(&window, &toast_timer, &msg);
                return;
            }
            log::info!("=== CHAIN SAVED: id='{}', name={:?}, instrument='{}', editing={:?} ===",
                chain.id.0, chain.description, chain.instrument, editing_index);
            let chain_id = chain.id.clone();
            if let Some(index) = editing_index {
                if let Some(current) = session.project.chains.get_mut(index) {
                    *current = chain;
                }
            } else {
                session.project.chains.push(chain);
            }
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
            *chain_draft.borrow_mut() = None;
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
        });
    }
    {
        let weak_window = window.as_weak();
        let chain_draft = chain_draft.clone();
        let toast_timer = toast_timer.clone();
        window.on_cancel_chain(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            *chain_draft.borrow_mut() = None;
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
        });
    }
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
pub(crate) fn stop_project_runtime(project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>) {
    if let Some(mut runtime) = project_runtime.borrow_mut().take() {
        runtime.stop();
    }
}
pub(crate) fn sync_project_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    session: &ProjectSession,
) -> Result<()> {
    let mut borrow = project_runtime.borrow_mut();
    if let Some(runtime) = borrow.as_mut() {
        validate_project(&session.project)?;
        runtime.sync_project(&session.project)?;
    }
    Ok(())
}
pub(crate) fn sync_live_chain_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    session: &ProjectSession,
    chain_id: &ChainId,
) -> Result<()> {
    log::debug!("sync_live_chain_runtime: chain_id='{}'", chain_id.0);
    let chain = session
        .project
        .chains
        .iter()
        .find(|c| &c.id == chain_id);
    let chain_enabled = chain.map(|c| c.enabled).unwrap_or(false);
    // If chain is being enabled and no runtime exists, create one
    if chain_enabled {
        let mut borrow = project_runtime.borrow_mut();
        if borrow.is_none() {
            *borrow = Some(ProjectRuntimeController::start(&session.project)?);
            return Ok(()); // start() already processes all enabled chains via sync_project
        }
        drop(borrow);
    }
    // Normal sync
    let mut borrow = project_runtime.borrow_mut();
    if let Some(runtime) = borrow.as_mut() {
        validate_project(&session.project)?;
        if let Some(chain) = chain {
            runtime.upsert_chain(&session.project, chain)?;
        } else {
            runtime.remove_chain(chain_id);
        }
        // If no chains are running, destroy runtime
        if !runtime.is_running() {
            *borrow = None;
        }
    }
    Ok(())
}
pub(crate) fn remove_live_chain_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    chain_id: &ChainId,
) {
    if let Some(runtime) = project_runtime.borrow_mut().as_mut() {
        runtime.remove_chain(chain_id);
    }
}
pub(crate) fn assign_new_block_ids(chain: &mut Chain) {
    for block in &mut chain.blocks {
        assign_new_block_ids_recursive(block, &chain.id);
    }
}
fn assign_new_block_ids_recursive(block: &mut AudioBlock, chain_id: &ChainId) {
    block.id = BlockId::generate_for_chain(chain_id);
    if let AudioBlockKind::Select(select) = &mut block.kind {
        for option in &mut select.options {
            assign_new_block_ids_recursive(option, chain_id);
        }
    }
}
pub(crate) fn system_language() -> String {
    let lang = std::env::var("LANG").unwrap_or_default();
    let base = lang.split('.').next().unwrap_or("");
    // "C", "POSIX", empty, or too short = not a real locale → fall back to English
    if base.is_empty() || base.len() < 2 || matches!(base, "C" | "POSIX") {
        return "en-US".to_string();
    }
    base.replace('_', "-")
}

/// Map a UI block index (which excludes hidden first Input and last Output) to the real chain.blocks index.
pub(crate) fn ui_index_to_real_block_index(chain: &Chain, ui_index: usize) -> usize {
    let first_input_idx = chain.blocks.iter().position(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
    let last_output_idx = chain.blocks.iter().rposition(|b| matches!(&b.kind, AudioBlockKind::Output(_)));
    let mut visible_count = 0;
    for (real_idx, _) in chain.blocks.iter().enumerate() {
        if Some(real_idx) == first_input_idx || Some(real_idx) == last_output_idx {
            continue; // hidden
        }
        if visible_count == ui_index {
            return real_idx;
        }
        visible_count += 1;
    }
    // If ui_index is past all visible blocks, return end (before last output)
    last_output_idx.unwrap_or(chain.blocks.len())
}
/// Register all callbacks on a freshly-created `ChainEditorWindow`.
/// Called each time the chain editor is opened so the window starts with clean state.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
mod tests {
    use super::{
        block_editor_data, block_parameter_items_for_editor, open_cli_project, parse_cli_args_from,
        SELECT_SELECTED_BLOCK_ID,
    };
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

    use super::ui_index_to_real_block_index;
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

    use super::insert_mode_to_index;
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

    use super::selected_device_index;
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

    use super::block_model_index;

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

    use super::block_type_index;

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