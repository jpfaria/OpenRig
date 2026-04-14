mod thumbnails;
mod plugin_info;

use anyhow::{anyhow, Result};

const SELECT_PATH_PREFIX: &str = "__select.";
const SELECT_SELECTED_BLOCK_ID: &str = "__select.selected_block_id";
use application::validate::validate_project;
use domain::ids::{BlockId, DeviceId, ChainId};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, AudioDeviceDescriptor,
    ProjectRuntimeController,
};
use infra_filesystem::{
    AppConfig, FilesystemStorage, GuiAudioDeviceSettings, GuiAudioSettings, RecentProjectEntry,
};
use infra_yaml::{
    load_chain_preset_file, save_chain_preset_file, ChainBlocksPreset,
    YamlProjectRepository,
};
use project::block::{
    build_audio_block_kind, schema_for_block_model, AudioBlock, AudioBlockKind,
};
use project::catalog::{
    model_brand, model_display_name, model_type_label, supported_block_models,
    supported_block_type, supported_block_types,
};
use project::device::DeviceSettings;
use project::param::{CurveEditorRole, ParameterDomain, ParameterSet, ParameterUnit, ParameterWidget};
use project::project::Project;
use project::block::{InputBlock, InputEntry, OutputBlock, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use slint::{Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::fmt::Display;
use std::rc::Rc;
use std::{
    cell::RefCell,
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};
use ui_openrig::{AppRuntimeMode, InteractionMode, UiRuntimeContext};
use ui_state::{block_drawer_state, block_family_for_kind, chain_routing_summary};
mod ui_state;
mod visual_config;
slint::include_modules!();
const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const DEFAULT_BUFFER_SIZE_FRAMES: u32 = 256;
const DEFAULT_BIT_DEPTH: u32 = 32;
const SUPPORTED_SAMPLE_RATES: &[u32] = &[44_100, 48_000, 88_200, 96_000];
const SUPPORTED_BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];
const SUPPORTED_BIT_DEPTHS: &[u32] = &[16, 24, 32];
fn log_gui_message(context: &str, message: &str) {
    log::info!("[adapter-gui] {context}: {message}");
}
fn log_gui_error(context: &str, error: impl Display) {
    log::error!("[adapter-gui] {context}: {error}");
}
fn show_child_window(parent_window: &slint::Window, child_window: &slint::Window) {
    let pos = parent_window.position();
    log::warn!("[UI] show_child_window: parent_pos=({},{})", pos.x, pos.y);
    child_window.set_position(slint::WindowPosition::Physical(
        slint::PhysicalPosition { x: pos.x + 40, y: pos.y + 40 },
    ));
    match child_window.show() {
        Ok(_) => log::warn!("[UI] show_child_window: success"),
        Err(e) => log::error!("[UI] show_child_window: FAILED: {e}"),
    }
}
fn refresh_input_devices(
    device_options_model: &Rc<VecModel<SharedString>>,
) -> Vec<AudioDeviceDescriptor> {
    let devices = list_input_device_descriptors().unwrap_or_default();
    let names: Vec<SharedString> = devices
        .iter()
        .map(|d| SharedString::from(d.name.as_str()))
        .collect();
    device_options_model.set_vec(names);
    devices
}
fn refresh_output_devices(
    device_options_model: &Rc<VecModel<SharedString>>,
) -> Vec<AudioDeviceDescriptor> {
    let devices = list_output_device_descriptors().unwrap_or_default();
    let names: Vec<SharedString> = devices
        .iter()
        .map(|d| SharedString::from(d.name.as_str()))
        .collect();
    device_options_model.set_vec(names);
    devices
}
fn ensure_devices_loaded(
    input: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
) {
    if input.borrow().is_empty() {
        *input.borrow_mut() = list_input_device_descriptors().unwrap_or_default();
    }
    if output.borrow().is_empty() {
        *output.borrow_mut() = list_output_device_descriptors().unwrap_or_default();
    }
}
fn use_inline_block_editor(window: &AppWindow) -> bool {
    window.get_fullscreen()
        || (window.get_touch_optimized()
            && window
                .get_interaction_mode_label()
                .to_string()
                .to_lowercase()
                .contains("touch"))
}
/// Sets a toast notification on the main window and starts the auto-dismiss timer.
/// Also sets `status_message` for backward compatibility with pages that still reference it.
fn set_status_with_toast(
    window: &AppWindow,
    toast_timer: &Timer,
    message: &str,
    level: &str,
) {
    window.set_status_message(message.into());
    window.set_toast_message(message.into());
    window.set_toast_level(level.into());
    if !message.is_empty() {
        match level {
            "error" => {
                log::error!("{}", message);
                eprintln!("[ERROR] {}", message);
            }
            "warning" => {
                log::warn!("{}", message);
                eprintln!("[WARN] {}", message);
            }
            _ => {
                log::info!("{}", message);
                eprintln!("[INFO] {}", message);
            }
        }
        let weak = window.as_weak();
        toast_timer.start(TimerMode::SingleShot, Duration::from_secs(3), move || {
            if let Some(window) = weak.upgrade() {
                window.set_toast_message("".into());
                window.set_toast_level("info".into());
                window.set_status_message("".into());
            }
        });
    }
}
fn set_status_error(window: &AppWindow, toast_timer: &Timer, message: &str) {
    set_status_with_toast(window, toast_timer, message, "error");
}
fn set_status_info(window: &AppWindow, toast_timer: &Timer, message: &str) {
    set_status_with_toast(window, toast_timer, message, "info");
}
fn set_status_warning(window: &AppWindow, toast_timer: &Timer, message: &str) {
    set_status_with_toast(window, toast_timer, message, "warning");
}
fn clear_status(window: &AppWindow, toast_timer: &Timer) {
    toast_timer.stop();
    window.set_status_message("".into());
    window.set_toast_message("".into());
    window.set_toast_level("info".into());
}
fn sync_block_editor_window(window: &AppWindow, block_editor_window: &BlockEditorWindow) {
    block_editor_window.set_block_type_options(window.get_block_type_options());
    block_editor_window.set_block_model_options(window.get_block_model_options());
    block_editor_window.set_block_model_option_labels(window.get_block_model_option_labels());
    block_editor_window.set_block_drawer_title(window.get_block_drawer_title());
    block_editor_window.set_block_drawer_confirm_label(window.get_block_drawer_confirm_label());
    block_editor_window.set_block_drawer_status_message(window.get_block_drawer_status_message());
    block_editor_window.set_block_drawer_edit_mode(window.get_block_drawer_edit_mode());
    block_editor_window.set_block_drawer_selected_type_index(
        window.get_block_drawer_selected_type_index(),
    );
    block_editor_window.set_block_drawer_selected_model_index(
        window.get_block_drawer_selected_model_index(),
    );
    block_editor_window.set_block_drawer_enabled(window.get_block_drawer_enabled());
    block_editor_window.set_block_parameter_items(window.get_block_parameter_items());
}
#[allow(clippy::too_many_arguments)]
fn schedule_block_editor_persist(
    timer: &Rc<Timer>,
    window_weak: slint::Weak<AppWindow>,
    block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    context: &'static str,
    auto_save: bool,
) {
    timer.stop();
    timer.start(TimerMode::SingleShot, Duration::from_millis(30), move || {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let Some(draft) = block_editor_draft.borrow().clone() else {
            return;
        };
        if draft.block_index.is_none() {
            return;
        }
        let devs_in = input_chain_devices.borrow();
        let devs_out = output_chain_devices.borrow();
        if let Err(error) = persist_block_editor_draft(
            &window,
            &draft,
            &block_parameter_items,
            &project_session,
            &project_chains,
            &project_runtime,
            &saved_project_snapshot,
            &project_dirty,
            &*devs_in,
            &*devs_out,
            false,
            auto_save,
        ) {
            log::error!("[adapter-gui] {context}: {error}");
            window.set_block_drawer_status_message(error.to_string().into());
        }
    });
}
#[allow(clippy::too_many_arguments)]
fn schedule_block_editor_persist_for_block_win(
    timer: &Rc<Timer>,
    block_win_weak: slint::Weak<BlockEditorWindow>,
    main_win_weak: slint::Weak<AppWindow>,
    block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    context: &'static str,
    auto_save: bool,
) {
    timer.stop();
    timer.start(TimerMode::SingleShot, Duration::from_millis(30), move || {
        // Only persist if the block window is still alive
        if block_win_weak.upgrade().is_none() {
            return;
        }
        let Some(main_window) = main_win_weak.upgrade() else {
            return;
        };
        let Some(draft) = block_editor_draft.borrow().clone() else {
            return;
        };
        if draft.block_index.is_none() {
            return;
        }
        let devs_in = input_chain_devices.borrow();
        let devs_out = output_chain_devices.borrow();
        if let Err(error) = persist_block_editor_draft(
            &main_window,
            &draft,
            &block_parameter_items,
            &project_session,
            &project_chains,
            &project_runtime,
            &saved_project_snapshot,
            &project_dirty,
            &*devs_in,
            &*devs_out,
            false,
            auto_save,
        ) {
            log::error!("[adapter-gui] {context}: {error}");
            main_window.set_block_drawer_status_message(error.to_string().into());
        }
    });
}
#[derive(Debug, Clone)]
struct ProjectPaths {
    default_config_path: PathBuf,
}
#[derive(Debug, Deserialize, Serialize, Default)]
struct AppConfigYaml {
    #[serde(default)]
    presets_path: Option<PathBuf>,
}
#[derive(Debug, Clone)]
struct ProjectSession {
    project: Project,
    project_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    presets_path: PathBuf,
}
#[derive(Debug, Clone)]
struct InputGroupDraft {
    device_id: Option<String>,
    channels: Vec<usize>,
    mode: ChainInputMode,
}
#[derive(Debug, Clone)]
struct OutputGroupDraft {
    device_id: Option<String>,
    channels: Vec<usize>,
    mode: ChainOutputMode,
}
#[derive(Debug, Clone)]
struct ChainDraft {
    editing_index: Option<usize>,
    name: String,
    instrument: String,
    inputs: Vec<InputGroupDraft>,
    outputs: Vec<OutputGroupDraft>,
    editing_input_index: Option<usize>,
    editing_output_index: Option<usize>,
    /// Which block in chain.blocks is being edited by the I/O groups window.
    /// None = editing the fixed chip (first input / last output).
    /// Some(idx) = editing a specific I/O block at chain.blocks[idx].
    editing_io_block_index: Option<usize>,
    /// True when a new input entry was added as placeholder and the input config
    /// window is open. If the user cancels, the placeholder should be removed.
    adding_new_input: bool,
    /// True when a new output entry was added as placeholder and the output config
    /// window is open. If the user cancels, the placeholder should be removed.
    adding_new_output: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedBlock {
    chain_index: usize,
    block_index: usize,
}
#[derive(Debug, Clone)]
struct BlockEditorDraft {
    chain_index: usize,
    block_index: Option<usize>,
    before_index: usize,
    instrument: String,
    effect_type: String,
    model_id: String,
    enabled: bool,
    is_select: bool,
}
/// Transient state for inserting an I/O block via the block type picker.
#[derive(Debug, Clone)]
struct IoBlockInsertDraft {
    chain_index: usize,
    before_index: usize,
    kind: String, // "input" or "output"
}
/// Transient state for editing an Insert block's send/return endpoints.
#[derive(Debug, Clone)]
struct InsertDraft {
    chain_index: usize,
    block_index: usize,
    send_device_id: Option<String>,
    send_channels: Vec<usize>,
    send_mode: ChainInputMode,
    return_device_id: Option<String>,
    return_channels: Vec<usize>,
    return_mode: ChainInputMode,
}
struct BlockEditorData {
    effect_type: String,
    model_id: String,
    params: ParameterSet,
    enabled: bool,
    is_select: bool,
    select_options: Vec<SelectOptionEditorItem>,
    selected_select_option_block_id: Option<String>,
}
struct SelectOptionEditorItem {
    block_id: String,
    label: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChainEditorMode {
    Create,
    Edit,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioSettingsMode {
    Gui,
    Project,
}
#[derive(Debug, Serialize)]
struct ConfigYaml {
    presets_path: String,
}
const UNTITLED_PROJECT_NAME: &str = "UNTITLED PROJECT";
struct BlockWindow {
    chain_index: usize,
    block_index: usize,
    window: BlockEditorWindow,
    #[allow(dead_code)]
    stream_timer: Option<Rc<Timer>>,
}
fn build_knob_overlays(knob_layout: &[block_core::KnobLayoutEntry], param_items: &[BlockParameterItem]) -> Vec<BlockKnobOverlay> {
    knob_layout
        .iter()
        .map(|info| {
            let found = param_items
                .iter()
                .find(|p| p.path.as_str() == info.param_key);
            let value = found.map(|p| p.numeric_value).unwrap_or(info.min);
            let label = found
                .map(|p| p.label.to_string().to_uppercase())
                .unwrap_or_else(|| info.param_key.to_uppercase());
            BlockKnobOverlay {
                path: info.param_key.into(),
                label: label.into(),
                svg_cx: info.svg_cx,
                svg_cy: info.svg_cy,
                svg_r: info.svg_r,
                value,
                min_val: info.min,
                max_val: info.max,
                step: info.step,
            }
        })
        .collect()
}
fn open_cli_project(path: &PathBuf) -> Result<ProjectSession> {
    if !path.exists() {
        anyhow::bail!("CLI project path does not exist: {:?}", path);
    }
    let config_path = resolve_project_config_path(path);
    load_project_session(path, &config_path)
}

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
    let saved_project_snapshot = Rc::new(RefCell::new(None::<String>));
    let project_dirty = Rc::new(RefCell::new(false));
    let open_block_windows: Rc<RefCell<Vec<BlockWindow>>> = Rc::new(RefCell::new(Vec::new()));
    let inline_stream_timer: Rc<RefCell<Option<Timer>>> = Rc::new(RefCell::new(None));
    let open_compact_window: Rc<RefCell<Option<(usize, slint::Weak<CompactChainViewWindow>)>>> = Rc::new(RefCell::new(None));
    let audio_settings_mode = Rc::new(RefCell::new(AudioSettingsMode::Gui));
    let input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>> = Rc::new(RefCell::new(
        if needs_audio_settings { list_input_device_descriptors().unwrap_or_default() } else { Vec::new() }
    ));
    let output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>> = Rc::new(RefCell::new(
        if needs_audio_settings { list_output_device_descriptors().unwrap_or_default() } else { Vec::new() }
    ));
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
    let chain_insert_window =
        ChainInsertWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let insert_send_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    let insert_return_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    let block_editor_window =
        BlockEditorWindow::new().map_err(|error| anyhow!(error.to_string()))?;
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
        let health_timer = Timer::default();
        health_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(2),
            move || {
                let Some(win) = weak_window.upgrade() else { return; };
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
    window.set_block_model_option_labels(ModelRc::from(block_model_option_labels.clone()));
    window.set_block_parameter_items(ModelRc::from(block_parameter_items.clone()));
    window.set_multi_slider_points(ModelRc::from(multi_slider_points.clone()));
    window.set_curve_editor_points(ModelRc::from(curve_editor_points.clone()));
    window.set_eq_band_curves(ModelRc::from(eq_band_curves.clone()));
    block_editor_window.set_block_type_options(ModelRc::from(block_type_options.clone()));
    block_editor_window.set_block_model_options(ModelRc::from(block_model_options.clone()));
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
    {
        let weak_window = window.as_weak();
        block_editor_window.on_choose_block_model(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_choose_block_model(index);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_close_block_drawer(move || {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_close_block_drawer();
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_save_block_drawer(move || {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_save_block_drawer();
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_delete_block_drawer(move || {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_delete_block_drawer();
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let plugin_info_window = plugin_info_window.clone();
        block_editor_window.on_show_plugin_info(move |effect_type, model_id| {
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
    {
        let weak_window = window.as_weak();
        block_editor_window.on_toggle_block_drawer_enabled(move || {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_block_drawer_enabled();
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_update_block_parameter_text(move |path, value| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_update_block_parameter_text(path, value);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_update_block_parameter_number(move |path, value| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_update_block_parameter_number(path, value);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_update_block_parameter_number_text(move |path, value| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_update_block_parameter_number_text(path, value);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_update_block_parameter_bool(move |path, value| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_update_block_parameter_bool(path, value);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_select_block_parameter_option(move |path, index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_block_parameter_option(path, index);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_pick_block_parameter_file(move |path| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_pick_block_parameter_file(path);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        block_editor_window.on_open_vst3_editor(move |model_id| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_open_vst3_editor(model_id);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        chain_input_window.on_select_device(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_chain_input_device(index);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        chain_input_window.on_toggle_channel(move |index, selected| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_chain_input_channel(index, selected);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        chain_output_window.on_select_device(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_chain_output_device(index);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        chain_output_window.on_toggle_channel(move |index, selected| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_chain_output_channel(index, selected);
            }
        });
    }
    {
        let chain_draft = chain_draft.clone();
        chain_output_window.on_select_output_mode(move |index| {
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                if let Some(gi) = draft.editing_output_index {
                    if let Some(output) = draft.outputs.get_mut(gi) {
                        output.mode = output_mode_from_index(index);
                        log::debug!("[select_output_mode] group={}, index={}, mode={:?}", gi, index, output.mode);
                    }
                }
            }
        });
    }
    project_settings_window.set_project_devices(ModelRc::from(project_devices.clone()));
    window.set_project_devices(ModelRc::from(project_devices.clone()));
    project_settings_window.set_sample_rate_options(window.get_sample_rate_options());
    project_settings_window.set_buffer_size_options(window.get_buffer_size_options());
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
    // --- ChainInsertWindow callbacks ---
    {
        let insert_draft = insert_draft.clone();
        let output_chain_devices = output_chain_devices.clone();
        let insert_send_channels = insert_send_channels.clone();
        chain_insert_window.on_select_send_device(move |index| {
            let devs_out = output_chain_devices.borrow();
            let Some(device) = devs_out.get(index as usize) else { return; };
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else { return; };
            draft.send_device_id = Some(device.id.clone());
            draft.send_channels.clear();
            let items = build_insert_send_channel_items(draft, &*devs_out);
            replace_channel_options(&insert_send_channels, items);
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let insert_send_channels = insert_send_channels.clone();
        chain_insert_window.on_toggle_send_channel(move |index, selected| {
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else { return; };
            let ch = index as usize;
            if selected {
                if !draft.send_channels.contains(&ch) {
                    draft.send_channels.push(ch);
                }
            } else {
                draft.send_channels.retain(|&c| c != ch);
            }
            if let Some(mut row) = insert_send_channels.row_data(index as usize) {
                row.selected = selected;
                insert_send_channels.set_row_data(index as usize, row);
            }
        });
    }
    {
        let insert_draft = insert_draft.clone();
        chain_insert_window.on_select_send_mode(move |index| {
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else { return; };
            draft.send_mode = insert_mode_from_index(index);
            log::debug!("[select_send_mode] index={}, mode={:?}", index, draft.send_mode);
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let insert_return_channels = insert_return_channels.clone();
        chain_insert_window.on_select_return_device(move |index| {
            let devs_in = input_chain_devices.borrow();
            let Some(device) = devs_in.get(index as usize) else { return; };
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else { return; };
            draft.return_device_id = Some(device.id.clone());
            draft.return_channels.clear();
            let items = build_insert_return_channel_items(draft, &*devs_in);
            replace_channel_options(&insert_return_channels, items);
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let insert_return_channels = insert_return_channels.clone();
        chain_insert_window.on_toggle_return_channel(move |index, selected| {
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else { return; };
            let ch = index as usize;
            if selected {
                if !draft.return_channels.contains(&ch) {
                    draft.return_channels.push(ch);
                }
            } else {
                draft.return_channels.retain(|&c| c != ch);
            }
            if let Some(mut row) = insert_return_channels.row_data(index as usize) {
                row.selected = selected;
                insert_return_channels.set_row_data(index as usize, row);
            }
        });
    }
    {
        let insert_draft = insert_draft.clone();
        chain_insert_window.on_select_return_mode(move |index| {
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else { return; };
            draft.return_mode = insert_mode_from_index(index);
            log::debug!("[select_return_mode] index={}, mode={:?}", index, draft.return_mode);
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_toggle_enabled(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(iw) = weak_insert_window.upgrade() else { return; };
            let draft_borrow = insert_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else { return; };
            let chain_idx = draft.chain_index;
            let block_idx = draft.block_index;
            drop(draft_borrow);
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            let Some(block) = chain.blocks.get_mut(block_idx) else { return; };
            block.enabled = !block.enabled;
            iw.set_block_enabled(block.enabled);
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("toggle insert block enabled: {e}");
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_delete_block(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(iw) = weak_insert_window.upgrade() else { return; };
            let draft_borrow = insert_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else { return; };
            let chain_idx = draft.chain_index;
            let block_idx = draft.block_index;
            drop(draft_borrow);
            *insert_draft.borrow_mut() = None;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            if block_idx < chain.blocks.len() {
                chain.blocks.remove(block_idx);
            }
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("delete insert block: {e}");
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            let _ = iw.hide();
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_save(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(iw) = weak_insert_window.upgrade() else { return; };
            let draft_borrow = insert_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else { return; };
            if draft.send_device_id.is_none() || draft.send_channels.is_empty() {
                iw.set_status_message("Selecione dispositivo e canais de envio.".into());
                return;
            }
            if draft.return_device_id.is_none() || draft.return_channels.is_empty() {
                iw.set_status_message("Selecione dispositivo e canais de retorno.".into());
                return;
            }
            let chain_idx = draft.chain_index;
            let block_idx = draft.block_index;
            let send_endpoint = project::block::InsertEndpoint {
                device_id: DeviceId(draft.send_device_id.clone().unwrap_or_default()),
                mode: draft.send_mode,
                channels: draft.send_channels.clone(),
            };
            let return_endpoint = project::block::InsertEndpoint {
                device_id: DeviceId(draft.return_device_id.clone().unwrap_or_default()),
                mode: draft.return_mode,
                channels: draft.return_channels.clone(),
            };
            drop(draft_borrow);
            *insert_draft.borrow_mut() = None;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                let _ = iw.hide();
                return;
            };
            let Some(chain) = session.project.chains.get_mut(chain_idx) else {
                let _ = iw.hide();
                return;
            };
            let Some(block) = chain.blocks.get_mut(block_idx) else {
                let _ = iw.hide();
                return;
            };
            if let AudioBlockKind::Insert(ref mut ib) = block.kind {
                ib.send = send_endpoint;
                ib.return_ = return_endpoint;
            }
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("insert save error: {e}");
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            iw.set_status_message("".into());
            let _ = iw.hide();
        });
    }
    {
        let insert_draft = insert_draft.clone();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_cancel(move || {
            *insert_draft.borrow_mut() = None;
            if let Some(iw) = weak_insert_window.upgrade() {
                iw.set_status_message("".into());
                let _ = iw.hide();
            }
        });
    }
    {
        let input_devices = input_devices.clone();
        window.on_toggle_input_device(move |index, selected| {
            toggle_device_row(&input_devices, index as usize, selected);
        });
    }
    {
        let project_devices = project_devices.clone();
        project_settings_window.on_toggle_project_device(move |index, selected| {
            toggle_device_row(&project_devices, index as usize, selected);
        });
    }
    {
        let output_devices = output_devices.clone();
        window.on_toggle_output_device(move |index, selected| {
            toggle_device_row(&output_devices, index as usize, selected);
        });
    }
    {
        let input_devices = input_devices.clone();
        window.on_update_input_sample_rate(move |index, value| {
            update_device_sample_rate(&input_devices, index as usize, value);
        });
    }
    {
        let input_devices = input_devices.clone();
        window.on_update_input_buffer_size(move |index, value| {
            update_device_buffer_size(&input_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        project_settings_window.on_update_project_sample_rate(move |index, value| {
            update_device_sample_rate(&project_devices, index as usize, value);
        });
    }
    {
        let output_devices = output_devices.clone();
        window.on_update_output_sample_rate(move |index, value| {
            update_device_sample_rate(&output_devices, index as usize, value);
        });
    }
    {
        let output_devices = output_devices.clone();
        window.on_update_output_buffer_size(move |index, value| {
            update_device_buffer_size(&output_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        project_settings_window.on_update_project_buffer_size(move |index, value| {
            update_device_buffer_size(&project_devices, index as usize, value);
        });
    }
    // Fullscreen inline project settings callbacks (mirror the ProjectSettingsWindow callbacks)
    {
        let project_devices = project_devices.clone();
        window.on_toggle_project_device(move |index, selected| {
            toggle_device_row(&project_devices, index as usize, selected);
        });
    }
    {
        let project_devices = project_devices.clone();
        window.on_update_project_sample_rate(move |index, value| {
            update_device_sample_rate(&project_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        window.on_update_project_buffer_size(move |index, value| {
            update_device_buffer_size(&project_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        project_settings_window.on_update_project_bit_depth(move |index, value| {
            update_device_bit_depth(&project_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        window.on_toggle_project_device(move |index, selected| {
            toggle_device_row(&project_devices, index as usize, selected);
        });
    }
    {
        let project_devices = project_devices.clone();
        window.on_update_project_sample_rate(move |index, value| {
            update_device_sample_rate(&project_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        window.on_update_project_buffer_size(move |index, value| {
            update_device_buffer_size(&project_devices, index as usize, value);
        });
    }
    {
        let project_devices = project_devices.clone();
        window.on_update_project_bit_depth(move |index, value| {
            update_device_bit_depth(&project_devices, index as usize, value);
        });
    }
    {
        let weak_window = window.as_weak();
        let input_devices = input_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_go_to_output_step(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            match selected_device_settings(&input_devices, "input") {
                Ok(devices) if !devices.is_empty() => {
                    clear_status(&window, &toast_timer);
                    window.set_wizard_step(1);
                }
                Ok(_) => {
                    set_status_warning(&window, &toast_timer, "Selecione pelo menos um input antes de continuar.");
                }
                Err(error) => {
                    set_status_error(&window, &toast_timer, &error.to_string());
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_go_to_input_step(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            clear_status(&window, &toast_timer);
            window.set_wizard_step(0);
        });
    }
    {
        let weak_window = window.as_weak();
        let input_devices = input_devices.clone();
        let output_devices = output_devices.clone();
        let project_devices = project_devices.clone();
        let audio_settings_mode = audio_settings_mode.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_save_audio_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            match *audio_settings_mode.borrow() {
                AudioSettingsMode::Gui => {
                    let input_devices = match selected_device_settings(&input_devices, "input") {
                        Ok(devices) => devices,
                        Err(error) => {
                            set_status_error(&window, &toast_timer, &error.to_string());
                            return;
                        }
                    };
                    let output_devices = match selected_device_settings(&output_devices, "output") {
                        Ok(devices) => devices,
                        Err(error) => {
                            set_status_error(&window, &toast_timer, &error.to_string());
                            return;
                        }
                    };
                    let settings = GuiAudioSettings {
                        input_devices,
                        output_devices,
                    };
                    if !settings.is_complete() {
                        set_status_warning(&window, &toast_timer, "Selecione pelo menos um input e um output antes de continuar.");
                        return;
                    }
                    match FilesystemStorage::save_gui_audio_settings(&settings) {
                        Ok(()) => {
                            // Update in-memory device settings and resync the
                            // audio runtime so changes take effect immediately.
                            // On Linux/JACK this will restart jackd if sample
                            // rate or buffer size changed.
                            if let Some(session) = project_session.borrow_mut().as_mut() {
                                let all_devices: Vec<_> = settings.input_devices.iter()
                                    .chain(settings.output_devices.iter())
                                    .collect();
                                session.project.device_settings = all_devices
                                    .into_iter()
                                    .map(|d| DeviceSettings {
                                        device_id: DeviceId(d.device_id.clone()),
                                        sample_rate: d.sample_rate,
                                        buffer_size_frames: d.buffer_size_frames,
                                        bit_depth: d.bit_depth,
                                    })
                                    .collect();
                                log::info!(
                                    "save_audio_settings(Gui): updated device_settings ({} entries)",
                                    session.project.device_settings.len()
                                );
                                for ds in &session.project.device_settings {
                                    log::info!(
                                        "  device='{}' sr={} buf={} bit={}",
                                        ds.device_id.0, ds.sample_rate, ds.buffer_size_frames, ds.bit_depth
                                    );
                                }
                                if let Err(e) = sync_project_runtime(&project_runtime, session) {
                                    set_status_error(&window, &toast_timer, &e.to_string());
                                    return;
                                }
                            }
                            clear_status(&window, &toast_timer);
                            window.set_show_audio_settings(false);
                        }
                        Err(error) => set_status_error(&window, &toast_timer, &error.to_string()),
                    }
                }
                AudioSettingsMode::Project => {
                    let project_device_settings =
                        match selected_device_settings(&project_devices, "device") {
                            Ok(devices) => devices,
                            Err(error) => {
                                set_status_error(&window, &toast_timer, &error.to_string());
                                return;
                            }
                        };
                    // Persist device settings to per-machine config
                    let gui_settings = GuiAudioSettings {
                        input_devices: project_device_settings.clone(),
                        output_devices: project_device_settings.clone(),
                    };
                    if let Err(e) = FilesystemStorage::save_gui_audio_settings(&gui_settings) {
                        log::warn!("failed to persist gui audio settings: {e}");
                    }
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                        return;
                    };
                    session.project.device_settings = project_device_settings
                        .into_iter()
                        .map(|device| DeviceSettings {
                            device_id: DeviceId(device.device_id),
                            sample_rate: device.sample_rate,
                            buffer_size_frames: device.buffer_size_frames,
                            bit_depth: device.bit_depth,
                        })
                        .collect();
                    log::info!(
                        "save_audio_settings(Project/main_window): updated device_settings ({} entries)",
                        session.project.device_settings.len()
                    );
                    for ds in &session.project.device_settings {
                        log::info!(
                            "  device='{}' sr={} buf={} bit={}",
                            ds.device_id.0, ds.sample_rate, ds.buffer_size_frames, ds.bit_depth
                        );
                    }
                    if let Err(error) = sync_project_runtime(&project_runtime, session) {
                        set_status_error(&window, &toast_timer, &error.to_string());
                        return;
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &*input_chain_devices.borrow(),
                        &*output_chain_devices.borrow(),
                    );
                    window.set_project_title(
                        project_title_for_path(session.project_path.as_ref(), &session.project)
                            .into(),
                    );
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                    clear_status(&window, &toast_timer);
                    window.set_show_project_chains(true);
                    window.set_show_chain_editor(false);
                    window.set_show_project_settings(false);
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_settings = project_settings_window.as_weak();
        let input_devices = input_devices.clone();
        let output_devices = output_devices.clone();
        let project_devices = project_devices.clone();
        let audio_settings_mode = audio_settings_mode.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        project_settings_window.on_save_audio_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = weak_settings.upgrade() else {
                return;
            };
            let project_device_settings = match selected_device_settings(&project_devices, "device")
            {
                Ok(devices) => devices,
                Err(error) => {
                    settings_window.set_status_message(error.to_string().into());
                    return;
                }
            };
            match *audio_settings_mode.borrow() {
                AudioSettingsMode::Gui => {
                    let input_devices = match selected_device_settings(&input_devices, "input") {
                        Ok(devices) => devices,
                        Err(error) => {
                            settings_window.set_status_message(error.to_string().into());
                            return;
                        }
                    };
                    let output_devices = match selected_device_settings(&output_devices, "output") {
                        Ok(devices) => devices,
                        Err(error) => {
                            settings_window.set_status_message(error.to_string().into());
                            return;
                        }
                    };
                    let settings = GuiAudioSettings {
                        input_devices,
                        output_devices,
                    };
                    if !settings.is_complete() {
                        settings_window.set_status_message(
                            "Selecione pelo menos um input e um output antes de continuar.".into(),
                        );
                        return;
                    }
                    match FilesystemStorage::save_gui_audio_settings(&settings) {
                        Ok(()) => {
                            settings_window.set_status_message("".into());
                            clear_status(&window, &toast_timer);
                            window.set_show_audio_settings(false);
                            let _ = settings_window.hide();
                        }
                        Err(error) => settings_window.set_status_message(error.to_string().into()),
                    }
                }
                AudioSettingsMode::Project => {
                    // Persist device settings to per-machine config
                    let gui_settings = GuiAudioSettings {
                        input_devices: project_device_settings.clone(),
                        output_devices: project_device_settings.clone(),
                    };
                    if let Err(e) = FilesystemStorage::save_gui_audio_settings(&gui_settings) {
                        log::warn!("failed to persist gui audio settings: {e}");
                    }
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        settings_window.set_status_message("Nenhum projeto carregado.".into());
                        return;
                    };
                    session.project.device_settings = project_device_settings
                        .into_iter()
                        .map(|device| DeviceSettings {
                            device_id: DeviceId(device.device_id),
                            sample_rate: device.sample_rate,
                            buffer_size_frames: device.buffer_size_frames,
                            bit_depth: device.bit_depth,
                        })
                        .collect();
                    log::info!(
                        "save_audio_settings(Project/settings_window): updated device_settings ({} entries)",
                        session.project.device_settings.len()
                    );
                    for ds in &session.project.device_settings {
                        log::info!(
                            "  device='{}' sr={} buf={} bit={}",
                            ds.device_id.0, ds.sample_rate, ds.buffer_size_frames, ds.bit_depth
                        );
                    }
                    if let Err(error) = sync_project_runtime(&project_runtime, session) {
                        settings_window.set_status_message(error.to_string().into());
                        return;
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &*input_chain_devices.borrow(),
                        &*output_chain_devices.borrow(),
                    );
                    window.set_project_title(
                        project_title_for_path(session.project_path.as_ref(), &session.project)
                            .into(),
                    );
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                    settings_window.set_status_message("".into());
                    clear_status(&window, &toast_timer);
                    window.set_show_project_settings(false);
                    let _ = settings_window.hide();
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let recent_projects = recent_projects.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_open_project_file(move || {
            log::info!("on_open_project_file triggered");
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            ensure_devices_loaded(&input_chain_devices, &output_chain_devices);
            let Some(path) = FileDialog::new()
                .add_filter("OpenRig Project", &["yaml", "yml"])
                .set_title("Abrir projeto")
                .pick_file()
            else {
                return;
            };
            log::info!("opening project file: {:?}", path);
            match load_project_session(&path, &resolve_project_config_path(&path)) {
                Ok(session) => {
                    let canonical_path = canonical_project_path(&path).unwrap_or(path.clone());
                    let title = project_title_for_path(Some(&canonical_path), &session.project);
                    let display_name = project_display_name(&session.project);
                    stop_project_runtime(&project_runtime);
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
                    recent_projects.set_vec(recent_project_items(
                        &app_config.borrow().recent_projects,
                        window.get_recent_project_search().as_str(),
                    ));
                    set_project_dirty(&window, &project_dirty, false);
                    clear_status(&window, &toast_timer);
                    window.set_project_title(title.into());
                    window.set_project_name_draft(
                        project_session
                            .borrow()
                            .as_ref()
                            .and_then(|session| session.project.name.clone())
                            .unwrap_or_default()
                            .into(),
                    );
                    window.set_project_path_label(
                        format!("Projeto: {}", canonical_path.display()).into(),
                    );
                    window.set_show_project_launcher(false);
                    window.set_show_project_setup(false);
                    window.set_show_project_chains(true);
                    window.set_show_chain_editor(false);
                    window.set_show_project_settings(false);
                }
                Err(error) => {
                    set_status_error(&window, &toast_timer, &error.to_string());
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_create_project_file(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            clear_status(&window, &toast_timer);
            window.set_project_name_draft("".into());
            window.set_show_project_launcher(false);
            window.set_show_project_setup(true);
            window.set_show_project_chains(false);
            window.set_show_chain_editor(false);
            window.set_show_project_settings(false);
        });
    }
    {
        let weak_window = window.as_weak();
        let project_paths = project_paths.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_confirm_new_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let name = window.get_project_name_draft().trim().to_string();
            if name.is_empty() {
                set_status_error(&window, &toast_timer, "O nome do projeto é obrigatório.");
                return;
            }
            ensure_devices_loaded(&input_chain_devices, &output_chain_devices);
            stop_project_runtime(&project_runtime);
            let mut session = create_new_project_session(&project_paths.default_config_path);
            session.project.name = Some(name.clone());
            replace_project_chains(
                &project_chains,
                &session.project,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
            *project_session.borrow_mut() = Some(session);
            *saved_project_snapshot.borrow_mut() = None;
            clear_status(&window, &toast_timer);
            set_project_dirty(&window, &project_dirty, true);
            window.set_project_title(name.into());
            window.set_project_path_label("Projeto em memória".into());
            window.set_show_project_setup(false);
            window.set_show_project_launcher(false);
            window.set_show_project_chains(true);
            window.set_show_chain_editor(false);
            window.set_show_project_settings(false);
        });
    }
    {
        let weak_window = window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_cancel_new_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            clear_status(&window, &toast_timer);
            window.set_show_project_setup(false);
            window.set_show_project_launcher(true);
        });
    }
    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let project_session = project_session.clone();
        let recent_projects = recent_projects.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let toast_timer = toast_timer.clone();
        window.on_save_project(move || {
            log::info!("on_save_project triggered");
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let project_path = if let Some(path) = session.project_path.clone() {
                path
            } else {
                let Some(path) = FileDialog::new()
                    .add_filter("OpenRig Project", &["yaml", "yml"])
                    .set_title("Salvar projeto")
                    .set_file_name("project.yaml")
                    .save_file()
                else {
                    return;
                };
                session.project_path = Some(path.clone());
                session.config_path = Some(resolve_project_config_path(&path));
                session.presets_path = path
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("presets");
                path
            };
            match save_project_session(session, &project_path) {
                Ok(()) => {
                    let canonical_path =
                        canonical_project_path(&project_path).unwrap_or(project_path.clone());
                    register_recent_project(
                        &mut app_config.borrow_mut(),
                        &canonical_path,
                        &project_display_name(&session.project),
                    );
                    let _ = FilesystemStorage::save_app_config(&app_config.borrow());
                    recent_projects.set_vec(recent_project_items(
                        &app_config.borrow().recent_projects,
                        window.get_recent_project_search().as_str(),
                    ));
                    window.set_project_title(
                        project_title_for_path(Some(&canonical_path), &session.project).into(),
                    );
                    window.set_project_name_draft(
                        session.project.name.clone().unwrap_or_default().into(),
                    );
                    window.set_project_path_label(
                        format!("Projeto: {}", project_path.display()).into(),
                    );
                    *saved_project_snapshot.borrow_mut() = project_session_snapshot(session).ok();
                    set_project_dirty(&window, &project_dirty, false);
                    clear_status(&window, &toast_timer);
                }
                Err(error) => {
                    set_status_error(&window, &toast_timer, &error.to_string());
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let recent_projects = recent_projects.clone();
        window.on_filter_recent_projects(move |query| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            recent_projects.set_vec(recent_project_items(
                &app_config.borrow().recent_projects,
                query.as_str(),
            ));
            window.set_recent_project_search(query);
        });
    }
    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let recent_projects = recent_projects.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_open_recent_project(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            ensure_devices_loaded(&input_chain_devices, &output_chain_devices);
            let Some(recent) = app_config
                .borrow()
                .recent_projects
                .get(index as usize)
                .cloned()
            else {
                set_status_error(&window, &toast_timer, "Projeto recente inválido.");
                return;
            };
            if !recent.is_valid {
                set_status_error(&window, &toast_timer, &recent .invalid_reason .unwrap_or_else(|| "Projeto inválido.".to_string()) );
                return;
            }
            let path = PathBuf::from(&recent.project_path);
            match load_project_session(&path, &resolve_project_config_path(&path)) {
                Ok(session) => {
                    let canonical_path = canonical_project_path(&path).unwrap_or(path.clone());
                    let title = project_title_for_path(Some(&canonical_path), &session.project);
                    let display_name = project_display_name(&session.project);
                    stop_project_runtime(&project_runtime);
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
                    recent_projects.set_vec(recent_project_items(
                        &app_config.borrow().recent_projects,
                        window.get_recent_project_search().as_str(),
                    ));
                    set_project_dirty(&window, &project_dirty, false);
                    clear_status(&window, &toast_timer);
                    window.set_project_title(title.into());
                    window.set_project_name_draft(
                        project_session
                            .borrow()
                            .as_ref()
                            .and_then(|session| session.project.name.clone())
                            .unwrap_or_default()
                            .into(),
                    );
                    window.set_project_path_label(
                        format!("Projeto: {}", canonical_path.display()).into(),
                    );
                    window.set_show_project_launcher(false);
                    window.set_show_project_chains(true);
                    window.set_show_chain_editor(false);
                    window.set_show_project_settings(false);
                }
                Err(error) => {
                    mark_recent_project_invalid(
                        &mut app_config.borrow_mut(),
                        &path,
                        &error.to_string(),
                    );
                    let _ = FilesystemStorage::save_app_config(&app_config.borrow());
                    recent_projects.set_vec(recent_project_items(
                        &app_config.borrow().recent_projects,
                        window.get_recent_project_search().as_str(),
                    ));
                    set_status_error(&window, &toast_timer, "Projeto inválido. Corrija ou remova da lista.");
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let app_config = app_config.clone();
        let recent_projects = recent_projects.clone();
        window.on_remove_recent_project(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut config = app_config.borrow_mut();
            if (index as usize) < config.recent_projects.len() {
                config.recent_projects.remove(index as usize);
                let _ = FilesystemStorage::save_app_config(&config);
                recent_projects.set_vec(recent_project_items(
                    &config.recent_projects,
                    window.get_recent_project_search().as_str(),
                ));
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_devices = project_devices.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let audio_settings_mode = audio_settings_mode.clone();
        let project_settings_window = project_settings_window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_configure_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = project_settings_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            project_devices.set_vec(build_project_device_rows(
                &fresh_input,
                &fresh_output,
                &session.project.device_settings,
            ));
            *audio_settings_mode.borrow_mut() = AudioSettingsMode::Project;
            window.set_project_name_draft(session.project.name.clone().unwrap_or_default().into());
            settings_window
                .set_project_name_draft(session.project.name.clone().unwrap_or_default().into());
            settings_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_project_settings(true);
            if fullscreen {
                // In fullscreen mode, render inline — set project-devices on main window
                window.set_project_devices(settings_window.get_project_devices());
            } else {
                show_child_window(window.window(), settings_window.window());
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_update_project_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            window.set_project_name_draft(value.clone());
            if let Some(session) = project_session.borrow_mut().as_mut() {
                let trimmed = value.trim();
                session.project.name = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_settings = project_settings_window.as_weak();
        let project_session = project_session.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        project_settings_window.on_update_project_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = weak_settings.upgrade() else {
                return;
            };
            window.set_project_name_draft(value.clone());
            settings_window.set_project_name_draft(value.clone());
            if let Some(session) = project_session.borrow_mut().as_mut() {
                let trimmed = value.trim();
                session.project.name = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_close_project_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            clear_status(&window, &toast_timer);
            window.set_show_project_settings(false);
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_settings = project_settings_window.as_weak();
        let toast_timer = toast_timer.clone();
        project_settings_window.on_close_project_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = weak_settings.upgrade() else {
                return;
            };
            settings_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_project_settings(false);
            let _ = settings_window.hide();
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let toast_timer = toast_timer.clone();
        window.on_save_chain_preset(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                set_status_error(&window, &toast_timer, "Chain inválida.");
                return;
            };
            let default_name = chain
                .description
                .clone()
                .unwrap_or_else(|| format!("chain_{}", index + 1))
                .replace(' ', "_")
                .to_lowercase();
            let Some(path) = FileDialog::new()
                .add_filter("OpenRig Preset", &["yaml", "yml"])
                .set_title("Salvar preset")
                .set_directory(&session.presets_path)
                .set_file_name(format!("{default_name}.yaml"))
                .save_file()
            else {
                return;
            };
            match save_chain_blocks_to_preset(chain, &path) {
                Ok(()) => set_status_info(&window, &toast_timer, "Preset salvo."),
                Err(error) => set_status_error(&window, &toast_timer, &error.to_string()),
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_configure_chain_preset(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let Some(path) = FileDialog::new()
                .add_filter("OpenRig Preset", &["yaml", "yml"])
                .set_title("Carregar preset na chain")
                .set_directory(&session.presets_path)
                .pick_file()
            else {
                return;
            };
            match load_preset_file(&path) {
                Ok(preset) => {
                    if let Some(chain) = session.project.chains.get_mut(index as usize) {
                        chain.blocks = preset.blocks;
                        assign_new_block_ids(chain);
                        let chain_id = chain.id.clone();
                        if let Err(error) =
                            sync_live_chain_runtime(&project_runtime, session, &chain_id)
                        {
                            set_status_error(&window, &toast_timer, &error.to_string());
                            return;
                        }
                        replace_project_chains(
                            &project_chains,
                            &session.project,
                            &*input_chain_devices.borrow(),
                            &*output_chain_devices.borrow(),
                        );
                        sync_project_dirty(
                            &window,
                            session,
                            &saved_project_snapshot,
                            &project_dirty,
                            auto_save,
                        );
                        clear_status(&window, &toast_timer);
                    }
                }
                Err(error) => set_status_error(&window, &toast_timer, &error.to_string()),
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let project_settings_window = project_settings_window.as_weak();
        let chain_editor_window = chain_editor_window.clone();
        let block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_back_to_launcher(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            if let Some(settings_window) = project_settings_window.upgrade() {
                let _ = settings_window.hide();
            }
            if let Some(editor_window) = chain_editor_window.borrow().as_ref() {
                let _ = editor_window.hide();
            }
            if let Some(editor_window) = block_editor_window.upgrade() {
                let _ = editor_window.hide();
            }
            stop_project_runtime(&project_runtime);
            *project_session.borrow_mut() = None;
            *saved_project_snapshot.borrow_mut() = None;
            replace_project_chains(
                &project_chains,
                &Project {
                    name: None,
                    device_settings: Vec::new(),
                    chains: Vec::new(),
                },
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
            clear_status(&window, &toast_timer);
            set_project_dirty(&window, &project_dirty, false);
            window.set_project_title("Projeto".into());
            window.set_project_name_draft("".into());
            window.set_project_path_label("".into());
            window.set_show_project_settings(false);
            window.set_show_chain_editor(false);
            window.set_show_project_chains(false);
            window.set_show_project_setup(false);
            window.set_show_project_launcher(true);
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_input_channels = chain_input_channels.clone();
        let chain_output_channels = chain_output_channels.clone();
        let chain_editor_window = chain_editor_window.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let toast_timer = toast_timer.clone();
        window.on_add_chain(move || {
            log::info!("on_add_chain triggered");
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            ensure_devices_loaded(&input_chain_devices, &output_chain_devices);
            let editor_window = match ChainEditorWindow::new() {
                Ok(w) => w,
                Err(e) => {
                    log::error!("Failed to create chain editor window: {e}");
                    return;
                }
            };
            setup_chain_editor_callbacks(
                &editor_window,
                window.as_weak(),
                chain_draft.clone(),
                project_session.clone(),
                project_chains.clone(),
                project_runtime.clone(),
                saved_project_snapshot.clone(),
                project_dirty.clone(),
                input_chain_devices.clone(),
                output_chain_devices.clone(),
                chain_input_device_options.clone(),
                chain_output_device_options.clone(),
                chain_input_channels.clone(),
                chain_output_channels.clone(),
                weak_input_window.clone(),
                weak_output_window.clone(),
                io_block_insert_draft.clone(),
                toast_timer.clone(),
                auto_save,
            );
            *chain_editor_window.borrow_mut() = Some(editor_window);
            let ce_borrow = chain_editor_window.borrow();
            let editor_window = ce_borrow.as_ref().unwrap();
            let borrow = project_session.borrow();
            let Some(session) = borrow.as_ref() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let devs_in = input_chain_devices.borrow();
            let devs_out = output_chain_devices.borrow();
            let draft = create_chain_draft(
                &session.project,
                &*devs_in,
                &*devs_out,
            );
            *chain_draft.borrow_mut() = Some(draft.clone());
            apply_chain_editor_labels(&window, &draft);
            apply_chain_io_groups(
                &window,
                &editor_window,
                &draft,
                &*devs_in,
                &*devs_out,
            );
            if let Some(input_group) = draft.inputs.first() {
                replace_channel_options(
                    &chain_input_channels,
                    build_input_channel_items(input_group, &draft, &session.project, &*devs_in),
                );
            }
            if let Some(output_group) = draft.outputs.first() {
                replace_channel_options(
                    &chain_output_channels,
                    build_output_channel_items(output_group, &*devs_out),
                );
            }
            window.set_chain_draft_name(draft.name.clone().into());
            editor_window.set_chain_name(draft.name.clone().into());
            editor_window.set_editor_title(window.get_chain_editor_title());
            editor_window.set_editor_save_label(window.get_chain_editor_save_label());
            editor_window.set_is_create_mode(true);
            editor_window.set_selected_instrument_index(instrument_string_to_index(&draft.instrument));
            window.set_selected_chain_input_device_index(selected_device_index(
                &*devs_in,
                draft.inputs.first().and_then(|i| i.device_id.as_deref()),
            ));
            window.set_selected_chain_output_device_index(selected_device_index(
                &*devs_out,
                draft.outputs.first().and_then(|o| o.device_id.as_deref()),
            ));
            editor_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(true);
            if fullscreen {
                window.set_chain_editor_input_groups(editor_window.get_input_groups());
                window.set_chain_editor_output_groups(editor_window.get_output_groups());
                window.set_chain_editor_is_create_mode(editor_window.get_is_create_mode());
                window.set_chain_editor_selected_instrument_index(editor_window.get_selected_instrument_index());
            } else {
                show_child_window(window.window(), editor_window.window());
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_input_channels = chain_input_channels.clone();
        let chain_output_channels = chain_output_channels.clone();
        let chain_editor_window = chain_editor_window.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let toast_timer = toast_timer.clone();
        window.on_configure_chain(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            ensure_devices_loaded(&input_chain_devices, &output_chain_devices);
            let editor_window = match ChainEditorWindow::new() {
                Ok(w) => w,
                Err(e) => {
                    log::error!("Failed to create chain editor window: {e}");
                    return;
                }
            };
            setup_chain_editor_callbacks(
                &editor_window,
                window.as_weak(),
                chain_draft.clone(),
                project_session.clone(),
                project_chains.clone(),
                project_runtime.clone(),
                saved_project_snapshot.clone(),
                project_dirty.clone(),
                input_chain_devices.clone(),
                output_chain_devices.clone(),
                chain_input_device_options.clone(),
                chain_output_device_options.clone(),
                chain_input_channels.clone(),
                chain_output_channels.clone(),
                weak_input_window.clone(),
                weak_output_window.clone(),
                io_block_insert_draft.clone(),
                toast_timer.clone(),
                auto_save,
            );
            *chain_editor_window.borrow_mut() = Some(editor_window);
            let ce_borrow = chain_editor_window.borrow();
            let editor_window = ce_borrow.as_ref().unwrap();
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                set_status_error(&window, &toast_timer, "Chain inválida.");
                return;
            };
            let devs_in = input_chain_devices.borrow();
            let devs_out = output_chain_devices.borrow();
            let draft = chain_draft_from_chain(index as usize, chain);
            if let Some(input_group) = draft.inputs.first() {
                replace_channel_options(
                    &chain_input_channels,
                    build_input_channel_items(input_group, &draft, &session.project, &*devs_in),
                );
            }
            if let Some(output_group) = draft.outputs.first() {
                replace_channel_options(
                    &chain_output_channels,
                    build_output_channel_items(output_group, &*devs_out),
                );
            }
            window.set_chain_draft_name(draft.name.clone().into());
            editor_window.set_chain_name(draft.name.clone().into());
            window.set_selected_chain_input_device_index(selected_device_index(
                &*devs_in,
                draft.inputs.first().and_then(|i| i.device_id.as_deref()),
            ));
            window.set_selected_chain_output_device_index(selected_device_index(
                &*devs_out,
                draft.outputs.first().and_then(|o| o.device_id.as_deref()),
            ));
            *chain_draft.borrow_mut() = Some(draft);
            if let Some(draft) = chain_draft.borrow().as_ref() {
                apply_chain_editor_labels(&window, draft);
                apply_chain_io_groups(
                    &window,
                    &editor_window,
                    draft,
                    &*devs_in,
                    &*devs_out,
                );
                editor_window.set_editor_title(window.get_chain_editor_title());
                editor_window.set_editor_save_label(window.get_chain_editor_save_label());
                editor_window.set_is_create_mode(false);
                editor_window.set_selected_instrument_index(instrument_string_to_index(&draft.instrument));
            }
            editor_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(true);
            if fullscreen {
                // In fullscreen mode, render inline — sync chain editor properties to main window
                window.set_chain_editor_input_groups(editor_window.get_input_groups());
                window.set_chain_editor_output_groups(editor_window.get_output_groups());
                window.set_chain_editor_is_create_mode(editor_window.get_is_create_mode());
                window.set_chain_editor_selected_instrument_index(editor_window.get_selected_instrument_index());
            } else {
                show_child_window(window.window(), editor_window.window());
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let toast_timer = toast_timer.clone();
        let open_compact_window = open_compact_window.clone();
        let vst3_editor_handles_for_compact = vst3_editor_handles.clone();
        let block_editor_draft = block_editor_draft.clone();
        window.on_open_compact_chain_view(move |chain_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            // In fullscreen mode, compact view is not available
            if fullscreen {
                return;
            }
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let ci = chain_index as usize;
            let Some(chain) = session.project.chains.get(ci) else {
                set_status_error(&window, &toast_timer, "Chain inválida.");
                return;
            };

            let compact_win = match CompactChainViewWindow::new() {
                Ok(w) => w,
                Err(e) => {
                    log::error!("failed to create compact chain view: {e}");
                    return;
                }
            };
            let title = chain
                .description
                .clone()
                .unwrap_or_else(|| format!("Chain {}", ci + 1));
            compact_win.set_chain_title(title.into());
            compact_win.set_chain_index(chain_index);
            compact_win.set_chain_enabled(chain.enabled);
            compact_win.set_block_type_options(ModelRc::from(Rc::new(VecModel::from(
                block_type_picker_items(&chain.instrument),
            ))));

            let blocks = build_compact_blocks(&session.project, ci);
            compact_win.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));

            // Store weak ref for refresh after block insert/save
            *open_compact_window.borrow_mut() = Some((ci, compact_win.as_weak()));

            // Wire toggle-enabled callback
            {
                let project_session = project_session.clone();
                let project_runtime = project_runtime.clone();
                let project_chains = project_chains.clone();
                let input_chain_devices = input_chain_devices.clone();
                let output_chain_devices = output_chain_devices.clone();
                let saved_project_snapshot = saved_project_snapshot.clone();
                let project_dirty = project_dirty.clone();
                let block_editor_draft = block_editor_draft.clone();
                let weak_main = window.as_weak();
                let weak_compact = compact_win.as_weak();
                let toast_timer = toast_timer.clone();
                compact_win.on_toggle_block_enabled(move |ci, bi| {
                    let Some(main_win) = weak_main.upgrade() else {
                        return;
                    };
                    let Some(cw) = weak_compact.upgrade() else {
                        return;
                    };
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        return;
                    };
                    let chain_idx = ci as usize;
                    let block_idx = bi as usize;
                    let Some(chain) = session.project.chains.get_mut(chain_idx) else {
                        return;
                    };
                    let Some(block) = chain.blocks.get_mut(block_idx) else {
                        return;
                    };
                    block.enabled = !block.enabled;
                    let new_enabled = block.enabled;
                    let chain_id = chain.id.clone();
                    // Keep block_editor_draft in sync to prevent stale persist from reverting
                    if let Some(draft) = block_editor_draft.borrow_mut().as_mut() {
                        if draft.chain_index == chain_idx && draft.block_index == Some(block_idx) {
                            draft.enabled = new_enabled;
                        }
                    }
                    if let Err(error) =
                        sync_live_chain_runtime(&project_runtime, session, &chain_id)
                    {
                        set_status_error(&main_win, &toast_timer, &error.to_string());
                        return;
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &*input_chain_devices.borrow(),
                        &*output_chain_devices.borrow(),
                    );
                    // Refresh compact blocks
                    let blocks = build_compact_blocks(&session.project, chain_idx);
                    cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
                    sync_project_dirty(
                        &main_win,
                        session,
                        &saved_project_snapshot,
                        &project_dirty,
                        auto_save,
                    );
                });
            }

            // Wire close callback
            {
                let weak_compact = compact_win.as_weak();
                compact_win.on_close_compact_view(move || {
                    if let Some(cw) = weak_compact.upgrade() {
                        cw.hide().ok();
                    }
                });
            }

            // Wire toggle-chain-enabled
            {
                let project_session = project_session.clone();
                let project_runtime = project_runtime.clone();
                let project_chains = project_chains.clone();
                let input_chain_devices = input_chain_devices.clone();
                let output_chain_devices = output_chain_devices.clone();
                let weak_main = window.as_weak();
                let weak_compact = compact_win.as_weak();
                let toast_timer = toast_timer.clone();
                compact_win.on_toggle_chain_enabled(move |ci| {
                    let Some(main_win) = weak_main.upgrade() else { return; };
                    let Some(cw) = weak_compact.upgrade() else { return; };
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else { return; };
                    let chain_idx = ci as usize;
                    let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
                    let will_enable = !chain.enabled;
                    chain.enabled = will_enable;
                    let chain_id = chain.id.clone();
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                        set_status_error(&main_win, &toast_timer, &error.to_string());
                        return;
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    cw.set_chain_enabled(will_enable);
                });
            }

            // Wire choose-block-model
            {
                let project_session = project_session.clone();
                let project_runtime = project_runtime.clone();
                let project_chains = project_chains.clone();
                let input_chain_devices = input_chain_devices.clone();
                let output_chain_devices = output_chain_devices.clone();
                let saved_project_snapshot = saved_project_snapshot.clone();
                let project_dirty = project_dirty.clone();
                let weak_main = window.as_weak();
                let weak_compact = compact_win.as_weak();
                let toast_timer = toast_timer.clone();
                compact_win.on_choose_block_model(move |ci, bi, mi| {
                    let Some(main_win) = weak_main.upgrade() else { return; };
                    let Some(cw) = weak_compact.upgrade() else { return; };
                    let chain_idx = ci as usize;
                    let block_idx = bi as usize;
                    let model_idx = mi as usize;

                    // Get the instrument to filter models
                    let instrument = {
                        let session_borrow = project_session.borrow();
                        let Some(session) = session_borrow.as_ref() else { return; };
                        let Some(chain) = session.project.chains.get(chain_idx) else { return; };
                        chain.instrument.clone()
                    };

                    // Get the effect type from the current block
                    let effect_type = {
                        let session_borrow = project_session.borrow();
                        let Some(session) = session_borrow.as_ref() else { return; };
                        let Some(chain) = session.project.chains.get(chain_idx) else { return; };
                        let Some(block) = chain.blocks.get(block_idx) else { return; };
                        let Some(data) = block_editor_data(block) else { return; };
                        data.effect_type.clone()
                    };

                    let models = block_model_picker_items(&effect_type, &instrument);
                    let Some(model) = models.get(model_idx) else { return; };
                    let new_model_id = model.model_id.to_string();
                    let new_effect_type = model.effect_type.to_string();

                    // Build new block kind with default params
                    let new_params = block_core::param::ParameterSet::default();
                    let schema = match project::block::schema_for_block_model(&new_effect_type, &new_model_id) {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("compact choose-model schema error: {e}");
                            return;
                        }
                    };
                    let normalized = match new_params.normalized_against(&schema) {
                        Ok(p) => p,
                        Err(e) => {
                            log::error!("compact choose-model normalize error: {e}");
                            return;
                        }
                    };
                    let kind = match project::block::build_audio_block_kind(&new_effect_type, &new_model_id, normalized) {
                        Ok(k) => k,
                        Err(e) => {
                            log::error!("compact choose-model build error: {e}");
                            return;
                        }
                    };

                    // Apply to project
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else { return; };
                    let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
                    let Some(block) = chain.blocks.get_mut(block_idx) else { return; };
                    let block_id = block.id.clone();
                    let enabled = block.enabled;
                    block.kind = kind;
                    block.id = block_id;
                    block.enabled = enabled;

                    let chain_id = chain.id.clone();
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                        set_status_error(&main_win, &toast_timer, &error.to_string());
                        return;
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    let blocks = build_compact_blocks(&session.project, chain_idx);
                    cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
                    sync_project_dirty(&main_win, session, &saved_project_snapshot, &project_dirty, auto_save);
                });
            }

            // Wire choose-block-type — when user picks a type from the compact view picker
            {
                let weak_main = window.as_weak();
                compact_win.on_choose_block_type(move |ci, before, type_index| {
                    log::info!("[compact] choose-block-type: chain={}, before={}, type_index={}", ci, before, type_index);
                    let Some(main_win) = weak_main.upgrade() else { return; };
                    // Trigger the full insert flow on the main window (sets up draft + opens editor)
                    main_win.invoke_start_block_insert(ci, before);
                    // Select the type that was chosen
                    main_win.invoke_choose_block_type(type_index);
                });
            }

            // Wire remove-block
            {
                let project_session = project_session.clone();
                let weak_main = window.as_weak();
                let weak_compact = compact_win.as_weak();
                let project_chains = project_chains.clone();
                let project_runtime = project_runtime.clone();
                let saved_project_snapshot = saved_project_snapshot.clone();
                let project_dirty = project_dirty.clone();
                let input_chain_devices = input_chain_devices.clone();
                let output_chain_devices = output_chain_devices.clone();
                let _toast_timer = toast_timer.clone();
                compact_win.on_remove_block(move |ci, bi| {
                    log::info!("[compact] remove-block: chain={}, block={}", ci, bi);
                    let Some(main_win) = weak_main.upgrade() else { return; };
                    let Some(cw) = weak_compact.upgrade() else { return; };
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else { return; };
                    let chain_idx = ci as usize;
                    let block_idx = bi as usize;
                    let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
                    if block_idx >= chain.blocks.len() { return; }
                    chain.blocks.remove(block_idx);
                    let chain_id = chain.id.clone();
                    if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                        log::error!("[compact] remove-block runtime sync: {}", e);
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    let blocks = build_compact_blocks(&session.project, chain_idx);
                    cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
                    sync_project_dirty(&main_win, session, &saved_project_snapshot, &project_dirty, auto_save);
                });
            }

            // Wire reorder-block — resolve real indices from CompactBlockItem.block_index
            {
                let project_session = project_session.clone();
                let weak_main = window.as_weak();
                let weak_compact = compact_win.as_weak();
                let project_chains = project_chains.clone();
                let project_runtime = project_runtime.clone();
                let saved_project_snapshot = saved_project_snapshot.clone();
                let project_dirty = project_dirty.clone();
                let input_chain_devices = input_chain_devices.clone();
                let output_chain_devices = output_chain_devices.clone();
                compact_win.on_reorder_block(move |ci, compact_from, compact_before| {
                    let Some(main_win) = weak_main.upgrade() else { return; };
                    let Some(cw) = weak_compact.upgrade() else { return; };
                    // Look up real chain.blocks indices from the Slint compact model
                    let compact_model = cw.get_compact_blocks();
                    let compact_len = compact_model.row_count();
                    let from_pos = compact_from as usize;
                    if from_pos >= compact_len { return; }
                    let from_index = compact_model.row_data(from_pos)
                        .map(|item| item.block_index as usize)
                        .unwrap_or(0);
                    let before_pos = compact_before as usize;
                    let real_before = if before_pos < compact_len {
                        compact_model.row_data(before_pos)
                            .map(|item| item.block_index as usize)
                            .unwrap_or(0)
                    } else {
                        // "after last compact block" → one position after last compact block's real index
                        compact_model.row_data(compact_len - 1)
                            .map(|item| item.block_index as usize + 1)
                            .unwrap_or(0)
                    };
                    log::info!("[compact] reorder-block: compact_from={}, compact_before={}, real_from={}, real_before={}", compact_from, compact_before, from_index, real_before);
                    if real_before == from_index || real_before == from_index + 1 { return; }
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else { return; };
                    let chain_idx = ci as usize;
                    let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
                    let block_count = chain.blocks.len();
                    if from_index >= block_count { return; }
                    let block = chain.blocks.remove(from_index);
                    let mut normalized_before = real_before;
                    if normalized_before > from_index { normalized_before -= 1; }
                    let insert_at = normalized_before.min(chain.blocks.len());
                    chain.blocks.insert(insert_at, block);
                    let chain_id = chain.id.clone();
                    if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                        log::error!("[compact] reorder-block runtime sync: {}", e);
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    let blocks = build_compact_blocks(&session.project, chain_idx);
                    cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
                    sync_project_dirty(&main_win, session, &saved_project_snapshot, &project_dirty, auto_save);
                });
            }

            // Wire update-block-parameter-number (knobs)
            {
                let project_session = project_session.clone();
                let project_runtime = project_runtime.clone();
                let project_chains = project_chains.clone();
                let input_chain_devices = input_chain_devices.clone();
                let output_chain_devices = output_chain_devices.clone();
                let saved_project_snapshot = saved_project_snapshot.clone();
                let project_dirty = project_dirty.clone();
                let weak_main = window.as_weak();
                let weak_compact = compact_win.as_weak();
                let toast_timer = toast_timer.clone();
                compact_win.on_update_block_parameter_number(move |ci, bi, path, value| {
                    let Some(main_win) = weak_main.upgrade() else { return; };
                    let Some(cw) = weak_compact.upgrade() else { return; };
                    let chain_idx = ci as usize;
                    let block_idx = bi as usize;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else { return; };
                    let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
                    let Some(block) = chain.blocks.get_mut(block_idx) else { return; };
                    // Update the parameter in the block
                    if let AudioBlockKind::Core(ref mut core) = block.kind {
                        core.params.insert(path.as_str(), domain::value_objects::ParameterValue::Float(value));
                    }
                    // Rebuild block kind with updated params
                    let Some(data) = block_editor_data(block) else { return; };
                    let params_set = data.params.clone();
                    match project::block::build_audio_block_kind(&data.effect_type, &data.model_id, params_set) {
                        Ok(kind) => {
                            let id = block.id.clone();
                            let enabled = block.enabled;
                            block.kind = kind;
                            block.id = id;
                            block.enabled = enabled;
                        }
                        Err(e) => {
                            log::error!("[compact] update param error: {e}");
                            return;
                        }
                    }
                    let chain_id = chain.id.clone();
                    if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                        set_status_error(&main_win, &toast_timer, &e.to_string());
                        return;
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    let blocks = build_compact_blocks(&session.project, chain_idx);
                    cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
                    sync_project_dirty(&main_win, session, &saved_project_snapshot, &project_dirty, auto_save);
                });
            }

            // Wire select-block-parameter-option (enums)
            {
                let project_session = project_session.clone();
                let project_runtime = project_runtime.clone();
                let project_chains = project_chains.clone();
                let input_chain_devices = input_chain_devices.clone();
                let output_chain_devices = output_chain_devices.clone();
                let saved_project_snapshot = saved_project_snapshot.clone();
                let project_dirty = project_dirty.clone();
                let weak_main = window.as_weak();
                let weak_compact = compact_win.as_weak();
                let toast_timer = toast_timer.clone();
                compact_win.on_select_block_parameter_option(move |ci, bi, path, option_index| {
                    let Some(main_win) = weak_main.upgrade() else { return; };
                    let Some(cw) = weak_compact.upgrade() else { return; };
                    let chain_idx = ci as usize;
                    let block_idx = bi as usize;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else { return; };
                    let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
                    let Some(block) = chain.blocks.get_mut(block_idx) else { return; };
                    // Get the option value from the schema
                    let Some(data) = block_editor_data(block) else { return; };
                    let schema = match project::block::schema_for_block_model(&data.effect_type, &data.model_id) {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let Some(param_spec) = schema.parameters.iter().find(|p| p.path == path.as_str()) else { return; };
                    let option_value = match &param_spec.domain {
                        block_core::param::ParameterDomain::Enum { options } => {
                            options.get(option_index as usize).map(|o| o.value.clone())
                        }
                        _ => None,
                    };
                    let Some(value) = option_value else { return; };
                    // Update param
                    if let AudioBlockKind::Core(ref mut core) = block.kind {
                        core.params.insert(path.as_str(), domain::value_objects::ParameterValue::String(value));
                    }
                    // Rebuild
                    let Some(data) = block_editor_data(block) else { return; };
                    let params_set = data.params.clone();
                    match project::block::build_audio_block_kind(&data.effect_type, &data.model_id, params_set) {
                        Ok(kind) => {
                            let id = block.id.clone();
                            let enabled = block.enabled;
                            block.kind = kind;
                            block.id = id;
                            block.enabled = enabled;
                        }
                        Err(e) => {
                            log::error!("[compact] select option error: {e}");
                            return;
                        }
                    }
                    let chain_id = chain.id.clone();
                    if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                        set_status_error(&main_win, &toast_timer, &e.to_string());
                        return;
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    let blocks = build_compact_blocks(&session.project, chain_idx);
                    cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
                    sync_project_dirty(&main_win, session, &saved_project_snapshot, &project_dirty, auto_save);
                });
            }

            // Wire update-block-parameter-bool (bool toggles like mute_signal)
            {
                let project_session = project_session.clone();
                let project_runtime = project_runtime.clone();
                let project_chains = project_chains.clone();
                let input_chain_devices = input_chain_devices.clone();
                let output_chain_devices = output_chain_devices.clone();
                let saved_project_snapshot = saved_project_snapshot.clone();
                let project_dirty = project_dirty.clone();
                let weak_main = window.as_weak();
                let weak_compact = compact_win.as_weak();
                let toast_timer = toast_timer.clone();
                compact_win.on_update_block_parameter_bool(move |ci, bi, path, value| {
                    let Some(main_win) = weak_main.upgrade() else { return; };
                    let Some(cw) = weak_compact.upgrade() else { return; };
                    let chain_idx = ci as usize;
                    let block_idx = bi as usize;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else { return; };
                    let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
                    let Some(block) = chain.blocks.get_mut(block_idx) else { return; };
                    // Update the parameter in the block
                    if let AudioBlockKind::Core(ref mut core) = block.kind {
                        core.params.insert(path.as_str(), domain::value_objects::ParameterValue::Bool(value));
                    }
                    // Rebuild block kind with updated params
                    let Some(data) = block_editor_data(block) else { return; };
                    let params_set = data.params.clone();
                    match project::block::build_audio_block_kind(&data.effect_type, &data.model_id, params_set) {
                        Ok(kind) => {
                            let id = block.id.clone();
                            let enabled = block.enabled;
                            block.kind = kind;
                            block.id = id;
                            block.enabled = enabled;
                        }
                        Err(e) => {
                            log::error!("[compact] update bool param error: {e}");
                            return;
                        }
                    }
                    let chain_id = chain.id.clone();
                    if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                        set_status_error(&main_win, &toast_timer, &e.to_string());
                        return;
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    let blocks = build_compact_blocks(&session.project, chain_idx);
                    cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
                    sync_project_dirty(&main_win, session, &saved_project_snapshot, &project_dirty, auto_save);
                });
            }

            // Wire open-block-detail (click on model select opens full editor)
            {
                let weak_main = window.as_weak();
                let project_session_detail = project_session.clone();
                compact_win.on_open_block_detail(move |ci, bi| {
                    let Some(main_win) = weak_main.upgrade() else { return; };
                    // bi is a real block index from CompactBlockItem — convert to UI index
                    // because on_select_chain_block now expects UI indices
                    let session_borrow = project_session_detail.borrow();
                    let ui_bi = if let Some(session) = session_borrow.as_ref() {
                        if let Some(chain) = session.project.chains.get(ci as usize) {
                            real_block_index_to_ui(chain, bi as usize)
                                .map(|i| i as i32)
                                .unwrap_or(bi)
                        } else {
                            bi
                        }
                    } else {
                        bi
                    };
                    main_win.invoke_select_chain_block(ci, ui_bi);
                    let _ = main_win.window().show();
                });
            }

            // Stream polling timer — updates stream_data for enabled utility blocks
            {
                let weak_cw = compact_win.as_weak();
                let project_runtime_poll = project_runtime.clone();
                let stream_timer = Timer::default();
                stream_timer.start(
                    slint::TimerMode::Repeated,
                    std::time::Duration::from_millis(80),
                    move || {
                        let Some(cw) = weak_cw.upgrade() else { return; };
                        let rt_borrow = project_runtime_poll.borrow();
                        let Some(rt) = rt_borrow.as_ref() else { return; };
                        let compact_blocks = cw.get_compact_blocks();
                        for i in 0..compact_blocks.row_count() {
                            if let Some(mut item) = compact_blocks.row_data(i) {
                                if item.effect_type == "utility" {
                                    let stream_data = if item.enabled {
                                        let bid = domain::ids::BlockId(item.block_id.to_string());
                                        let kind: slint::SharedString = project::catalog::model_stream_kind(item.effect_type.as_str(), item.model_id.as_str()).into();
                                        if let Some(entries) = rt.poll_stream(&bid) {
                                            let slint_entries: Vec<BlockStreamEntry> = entries.iter().map(|e| BlockStreamEntry {
                                                key: e.key.clone().into(),
                                                value: e.value,
                                                text: e.text.clone().into(),
                                                peak: e.peak,
                                            }).collect();
                                            BlockStreamData {
                                                active: true,
                                                stream_kind: kind,
                                                entries: ModelRc::from(Rc::new(VecModel::from(slint_entries))),
                                            }
                                        } else {
                                            BlockStreamData {
                                                active: false,
                                                stream_kind: kind,
                                                entries: ModelRc::default(),
                                            }
                                        }
                                    } else {
                                        // Disabled utility block — clear stream so parameters become visible
                                        BlockStreamData { active: false, stream_kind: "".into(), entries: ModelRc::default() }
                                    };
                                    item.stream_data = stream_data;
                                    compact_blocks.set_row_data(i, item);
                                }
                            }
                        }
                    },
                );
                // Timer lives as long as compact_win (dropped when window closes)
                std::mem::forget(stream_timer);
            }

            // Wire configure-input/output — delegate to the main window's existing handlers
            {
                let weak_main = window.as_weak();
                compact_win.on_configure_input(move |ci| {
                    log::warn!("[compact] on_configure_input fired, chain_index={}", ci);
                    if let Some(main_win) = weak_main.upgrade() {
                        log::warn!("[compact] main_win upgrade OK, invoking configure_chain_input");
                        main_win.invoke_configure_chain_input(ci);
                    } else {
                        log::warn!("[compact] main_win upgrade FAILED");
                    }
                });
            }
            {
                let weak_main = window.as_weak();
                compact_win.on_configure_output(move |ci| {
                    log::warn!("[compact] on_configure_output fired, chain_index={}", ci);
                    if let Some(main_win) = weak_main.upgrade() {
                        log::warn!("[compact] main_win upgrade OK, invoking configure_chain_output");
                        main_win.invoke_configure_chain_output(ci);
                    } else {
                        log::warn!("[compact] main_win upgrade FAILED");
                    }
                });
            }

            {
                let vst3_handles = vst3_editor_handles_for_compact.clone();
                let vst3_sr = vst3_sample_rate;
                compact_win.on_open_plugin(move |model_id| {
                    match project::vst3_editor::open_vst3_editor(model_id.as_str(), vst3_sr) {
                        Ok(handle) => { vst3_handles.borrow_mut().push(handle); }
                        Err(e) => log::error!("[compact] failed to open VST3 editor '{}': {}", model_id, e),
                    }
                });
            }

            show_child_window(window.window(), compact_win.window());
        });
    }
    {
        let weak_window = window.as_weak();
        let chain_draft = chain_draft.clone();
        window.on_update_chain_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                draft.name = value.to_string();
                window.set_chain_draft_name(value);
            }
        });
    }
    {
        let chain_draft = chain_draft.clone();
        chain_input_window.on_select_input_mode(move |index| {
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                if let Some(gi) = draft.editing_input_index {
                    if let Some(input) = draft.inputs.get_mut(gi) {
                        input.mode = input_mode_from_index(index);
                        log::debug!("[select_input_mode] group={}, index={}, mode={:?}", gi, index, input.mode);
                    }
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let chain_editor_window_ref = chain_editor_window.clone();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        window.on_select_chain_input_device(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let Some(device) = fresh_input.get(index as usize) else {
                return;
            };
            let Some(gi) = draft.editing_input_index else {
                return;
            };
            // Mutate the group first
            {
                let Some(input_group) = draft.inputs.get_mut(gi) else {
                    return;
                };
                input_group.device_id = Some(device.id.clone());
                input_group.channels.clear();
            }
            // Now use immutable references
            if let Some(session) = project_session.borrow().as_ref() {
                if let Some(input_group) = draft.inputs.get(gi) {
                    replace_channel_options(
                        &chain_input_channels,
                        build_input_channel_items(input_group, draft, &session.project, &fresh_input),
                    );
                }
                if let Some(chain_window) = chain_editor_window_ref.borrow().as_ref() {
                    apply_chain_io_groups(
                        &window,
                        chain_window,
                        draft,
                        &fresh_input,
                        &fresh_output,
                    );
                }
            }
            let selected_index = selected_device_index(
                &fresh_input,
                draft.inputs.get(gi).and_then(|ig| ig.device_id.as_deref()),
            );
            window.set_selected_chain_input_device_index(selected_index);
            if let Some(input_window) = weak_input_window.upgrade() {
                input_window.set_selected_device_index(selected_index);
            }
            if let Some(chain_window) = chain_editor_window_ref.borrow().as_ref() {
                if chain_window.get_show_input_editor() {
                    chain_window.set_input_selected_device_index(selected_index);
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let chain_editor_window_ref = chain_editor_window.clone();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_output_channels = chain_output_channels.clone();
        window.on_select_chain_output_device(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let Some(device) = fresh_output.get(index as usize) else {
                return;
            };
            let Some(gi) = draft.editing_output_index else {
                return;
            };
            {
                let Some(output_group) = draft.outputs.get_mut(gi) else {
                    return;
                };
                output_group.device_id = Some(device.id.clone());
                output_group.channels.clear();
            }
            if project_session.borrow().as_ref().is_some() {
                if let Some(output_group) = draft.outputs.get(gi) {
                    replace_channel_options(
                        &chain_output_channels,
                        build_output_channel_items(output_group, &fresh_output),
                    );
                }
                if let Some(chain_window) = chain_editor_window_ref.borrow().as_ref() {
                    apply_chain_io_groups(
                        &window,
                        chain_window,
                        draft,
                        &fresh_input,
                        &fresh_output,
                    );
                }
            }
            let selected_index = selected_device_index(
                &fresh_output,
                draft.outputs.get(gi).and_then(|og| og.device_id.as_deref()),
            );
            window.set_selected_chain_output_device_index(selected_index);
            if let Some(output_window) = weak_output_window.upgrade() {
                output_window.set_selected_device_index(selected_index);
            }
            if let Some(chain_window) = chain_editor_window_ref.borrow().as_ref() {
                if chain_window.get_show_output_editor() {
                    chain_window.set_output_selected_device_index(selected_index);
                }
            }
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let chain_input_channels = chain_input_channels.clone();
        let weak_window = window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_toggle_chain_input_channel(move |index, selected| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let channel = index as usize;
            let Some(option) = chain_input_channels.row_data(index as usize) else {
                return;
            };
            if selected && !option.available && !option.selected {
                set_status_error(&window, &toast_timer, "Canal de entrada já está em uso por outra chain.");
                return;
            }
            let Some(gi) = draft.editing_input_index else {
                return;
            };
            {
                let Some(input_group) = draft.inputs.get_mut(gi) else {
                    return;
                };
                if selected {
                    if !input_group.channels.contains(&channel) {
                        input_group.channels.push(channel);
                        input_group.channels.sort_unstable();
                    }
                } else {
                    input_group.channels.retain(|current| *current != channel);
                }
            }
            if let Some(mut row) = chain_input_channels.row_data(index as usize) {
                row.selected = selected;
                chain_input_channels.set_row_data(index as usize, row);
            }
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let chain_output_channels = chain_output_channels.clone();
        window.on_toggle_chain_output_channel(move |index, selected| {
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let channel = index as usize;
            let Some(gi) = draft.editing_output_index else {
                return;
            };
            {
                let Some(output_group) = draft.outputs.get_mut(gi) else {
                    return;
                };
                if selected {
                    if !output_group.channels.contains(&channel) {
                        output_group.channels.push(channel);
                        output_group.channels.sort_unstable();
                    }
                } else {
                    output_group.channels.retain(|current| *current != channel);
                }
            }
            if let Some(mut row) = chain_output_channels.row_data(index as usize) {
                row.selected = selected;
                chain_output_channels.set_row_data(index as usize, row);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_groups_window = chain_input_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        window.on_configure_chain_input(move |index| {
            log::warn!("[UI] configure-chain-input clicked, chain_index={}", index);
            let Some(window) = weak_window.upgrade() else {
                log::warn!("[UI] configure-chain-input: window upgrade failed");
                return;
            };
            let Some(groups_window) = weak_groups_window.upgrade() else {
                log::warn!("[UI] configure-chain-input: groups_window upgrade failed");
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                return;
            };
            // Only show entries from the FIRST InputBlock (position 0 chip)
            let first_input = chain.first_input();
            let inputs: Vec<InputGroupDraft> = first_input
                .map(|ib| {
                    ib.entries.iter().map(|e| InputGroupDraft {
                        device_id: if e.device_id.0.is_empty() { None } else { Some(e.device_id.0.clone()) },
                        channels: e.channels.clone(),
                        mode: e.mode,
                    }).collect()
                })
                .unwrap_or_else(|| vec![InputGroupDraft {
                    device_id: None,
                    channels: Vec::new(),
                    mode: ChainInputMode::Mono,
                }]);
            let mut draft = chain_draft_from_chain(index as usize, chain);
            draft.inputs = inputs;
            let (input_items, _) =
                build_io_group_items(&draft, &fresh_input, &fresh_output);
            groups_window
                .set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
            groups_window.set_status_message("".into());
            groups_window.set_show_block_controls(false);
            *chain_draft.borrow_mut() = Some(draft);
            show_child_window(window.window(), groups_window.window());
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_groups_window = chain_output_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        window.on_configure_chain_output(move |index| {
            log::warn!("[UI] configure-chain-output clicked, chain_index={}", index);
            let Some(window) = weak_window.upgrade() else {
                log::warn!("[UI] configure-chain-output: window upgrade failed");
                return;
            };
            let Some(groups_window) = weak_groups_window.upgrade() else {
                log::warn!("[UI] configure-chain-output: groups_window upgrade failed");
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                return;
            };
            // Only show entries from the LAST OutputBlock (fixed output chip)
            let last_output = chain.last_output();
            let outputs: Vec<OutputGroupDraft> = last_output
                .map(|ob| {
                    ob.entries.iter().map(|e| OutputGroupDraft {
                        device_id: if e.device_id.0.is_empty() { None } else { Some(e.device_id.0.clone()) },
                        channels: e.channels.clone(),
                        mode: e.mode,
                    }).collect()
                })
                .unwrap_or_else(|| vec![OutputGroupDraft {
                    device_id: None,
                    channels: Vec::new(),
                    mode: ChainOutputMode::Stereo,
                }]);
            let mut draft = chain_draft_from_chain(index as usize, chain);
            draft.outputs = outputs;
            let (_, output_items) =
                build_io_group_items(&draft, &fresh_input, &fresh_output);
            groups_window
                .set_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
            groups_window.set_status_message("".into());
            groups_window.set_show_block_controls(false);
            *chain_draft.borrow_mut() = Some(draft);
            show_child_window(window.window(), groups_window.window());
        });
    }
    // --- ChainInputGroupsWindow callbacks ---
    {
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        chain_input_groups_window.on_edit_group(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(input_window) = weak_input_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let gi = group_index as usize;
            {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                draft.editing_input_index = Some(gi);
            }
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            if let Some(input_group) = draft.inputs.get(gi) {
                apply_chain_input_window_state(
                    &input_window,
                    input_group,
                    draft,
                    &session.project,
                    &fresh_input,
                    &chain_input_channels,
                );
            }
            show_child_window(window.window(), input_window.window());
        });
    }
    {
        let weak_groups_window = chain_input_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        chain_input_groups_window.on_remove_group(move |group_index| {
            let Some(groups_window) = weak_groups_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            // Fixed block (chip In/Out): must keep at least one entry
            if draft.editing_io_block_index.is_none() && draft.inputs.len() <= 1 {
                groups_window.set_status_message("É necessário pelo menos uma entrada.".into());
                return;
            }
            let gi = group_index as usize;
            if gi < draft.inputs.len() {
                draft.inputs.remove(gi);
                if draft.editing_input_index == Some(gi) {
                    draft.editing_input_index = None;
                } else if let Some(idx) = draft.editing_input_index {
                    if idx > gi {
                        draft.editing_input_index = Some(idx - 1);
                    }
                }
            }
            let (input_items, _) =
                build_io_group_items(draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            groups_window
                .set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_groups_window = chain_input_groups_window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        chain_input_groups_window.on_add_group(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(input_window) = weak_input_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let new_idx = {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                let idx = draft.inputs.len();
                draft.inputs.push(InputGroupDraft {
                    device_id: fresh_input.first().map(|d| d.id.clone()),
                    channels: Vec::new(),
                    mode: ChainInputMode::Mono,
                });
                draft.editing_input_index = Some(idx);
                draft.adding_new_input = true;
                if let Some(groups_window) = weak_groups_window.upgrade() {
                    let (input_items, _) =
                        build_io_group_items(draft, &fresh_input, &fresh_output);
                    groups_window
                        .set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
                }
                idx
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            if let Some(input_group) = draft.inputs.get(new_idx) {
                apply_chain_input_window_state(
                    &input_window,
                    input_group,
                    draft,
                    &session.project,
                    &fresh_input,
                    &chain_input_channels,
                );
            }
            show_child_window(window.window(), input_window.window());
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_groups_window = chain_input_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        chain_input_groups_window.on_save(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(groups_window) = weak_groups_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                groups_window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let draft = match chain_draft.borrow().clone() {
                Some(draft) => draft,
                None => {
                    groups_window.set_status_message("Nenhuma chain em edi\u{00E7}\u{00E3}o.".into());
                    return;
                }
            };
            if draft.inputs.is_empty() {
                groups_window.set_status_message("Adicione pelo menos uma entrada.".into());
                return;
            }
            for (i, input) in draft.inputs.iter().enumerate() {
                if input.device_id.is_none() {
                    groups_window.set_status_message(
                        format!("Entrada {}: selecione o dispositivo.", i + 1).into(),
                    );
                    return;
                }
                if input.channels.is_empty() {
                    groups_window.set_status_message(
                        format!("Entrada {}: selecione pelo menos um canal.", i + 1).into(),
                    );
                    return;
                }
            }
            let editing_index = draft.editing_index;
            let io_block_idx = draft.editing_io_block_index;

            // Build new entries from draft
            let new_entries: Vec<InputEntry> = draft.inputs.iter()
                .filter(|ig| ig.device_id.is_some() && !ig.channels.is_empty())
                .map(|ig| InputEntry {
                    device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
                    mode: ig.mode,
                    channels: ig.channels.clone(),
                }).collect();

            if let Some(chain_idx) = editing_index {
                if let Some(chain) = session.project.chains.get_mut(chain_idx) {
                    // Find target block: specific index or first InputBlock
                    let target_idx = io_block_idx.unwrap_or_else(|| {
                        chain.blocks.iter().position(|b| matches!(&b.kind, AudioBlockKind::Input(_))).unwrap_or(0)
                    });
                    if let Some(block) = chain.blocks.get_mut(target_idx) {
                        if let AudioBlockKind::Input(ref mut ib) = block.kind {
                            ib.entries = new_entries;
                        }
                    }
                    if let Err(msg) = chain.validate_channel_conflicts() {
                        groups_window.set_status_message(msg.into());
                        return;
                    }
                    let chain_id = chain.id.clone();
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                        groups_window.set_status_message(error.to_string().into());
                        return;
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                }
            }
            *chain_draft.borrow_mut() = None;
            groups_window.set_status_message("".into());
            let _ = groups_window.hide();
        });
    }
    {
        let weak_groups_window = chain_input_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        chain_input_groups_window.on_cancel(move || {
            let Some(groups_window) = weak_groups_window.upgrade() else {
                return;
            };
            *chain_draft.borrow_mut() = None;
            let _ = groups_window.hide();
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_groups_window = chain_input_groups_window.as_weak();
        chain_input_groups_window.on_toggle_enabled(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(gw) = weak_groups_window.upgrade() else { return; };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else { return; };
            let Some(chain_idx) = draft.editing_index else { return; };
            let Some(block_idx) = draft.editing_io_block_index else { return; };
            drop(draft_borrow);
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            let Some(block) = chain.blocks.get_mut(block_idx) else { return; };
            block.enabled = !block.enabled;
            gw.set_block_enabled(block.enabled);
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("toggle I/O block enabled: {e}");
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_groups_window = chain_input_groups_window.as_weak();
        chain_input_groups_window.on_delete_block(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(gw) = weak_groups_window.upgrade() else { return; };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else { return; };
            let Some(chain_idx) = draft.editing_index else { return; };
            let Some(block_idx) = draft.editing_io_block_index else { return; };
            drop(draft_borrow);
            *chain_draft.borrow_mut() = None;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            if block_idx < chain.blocks.len() {
                chain.blocks.remove(block_idx);
            }
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("delete I/O block: {e}");
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            let _ = gw.hide();
        });
    }
    // --- ChainOutputGroupsWindow callbacks ---
    {
        let weak_window = window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_output_channels = chain_output_channels.clone();
        chain_output_groups_window.on_edit_group(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(output_window) = weak_output_window.upgrade() else {
                return;
            };
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let gi = group_index as usize;
            {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                draft.editing_output_index = Some(gi);
            }
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(output_group) = draft.outputs.get(gi) {
                apply_chain_output_window_state(
                    &output_window,
                    output_group,
                    &fresh_output,
                    &chain_output_channels,
                );
            }
            show_child_window(window.window(), output_window.window());
        });
    }
    {
        let weak_groups_window = chain_output_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        chain_output_groups_window.on_remove_group(move |group_index| {
            let Some(groups_window) = weak_groups_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            // Fixed block (chip In/Out): must keep at least one entry
            if draft.editing_io_block_index.is_none() && draft.outputs.len() <= 1 {
                groups_window.set_status_message("É necessário pelo menos uma saída.".into());
                return;
            }
            let gi = group_index as usize;
            if gi < draft.outputs.len() {
                draft.outputs.remove(gi);
                if draft.editing_output_index == Some(gi) {
                    draft.editing_output_index = None;
                } else if let Some(idx) = draft.editing_output_index {
                    if idx > gi {
                        draft.editing_output_index = Some(idx - 1);
                    }
                }
            }
            let (_, output_items) =
                build_io_group_items(draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            groups_window
                .set_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_groups_window = chain_output_groups_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_output_channels = chain_output_channels.clone();
        chain_output_groups_window.on_add_group(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(output_window) = weak_output_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let new_idx = {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                let idx = draft.outputs.len();
                draft.outputs.push(OutputGroupDraft {
                    device_id: fresh_output.first().map(|d| d.id.clone()),
                    channels: Vec::new(),
                    mode: ChainOutputMode::Stereo,
                });
                draft.editing_output_index = Some(idx);
                draft.adding_new_output = true;
                if let Some(groups_window) = weak_groups_window.upgrade() {
                    let (_, output_items) =
                        build_io_group_items(draft, &fresh_input, &fresh_output);
                    groups_window
                        .set_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
                }
                idx
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(output_group) = draft.outputs.get(new_idx) {
                apply_chain_output_window_state(
                    &output_window,
                    output_group,
                    &fresh_output,
                    &chain_output_channels,
                );
            }
            show_child_window(window.window(), output_window.window());
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_groups_window = chain_output_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        chain_output_groups_window.on_save(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(groups_window) = weak_groups_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                groups_window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let draft = match chain_draft.borrow().clone() {
                Some(draft) => draft,
                None => {
                    groups_window.set_status_message("Nenhuma chain em edi\u{00E7}\u{00E3}o.".into());
                    return;
                }
            };
            if draft.outputs.is_empty() {
                groups_window.set_status_message("Adicione pelo menos uma sa\u{00ED}da.".into());
                return;
            }
            for (i, output) in draft.outputs.iter().enumerate() {
                if output.device_id.is_none() {
                    groups_window.set_status_message(
                        format!("Sa\u{00ED}da {}: selecione o dispositivo.", i + 1).into(),
                    );
                    return;
                }
                if output.channels.is_empty() {
                    groups_window.set_status_message(
                        format!("Sa\u{00ED}da {}: selecione pelo menos um canal.", i + 1).into(),
                    );
                    return;
                }
            }
            let editing_index = draft.editing_index;
            let io_block_idx = draft.editing_io_block_index;

            // Build new entries from draft
            let new_entries: Vec<OutputEntry> = draft.outputs.iter()
                .filter(|og| og.device_id.is_some() && !og.channels.is_empty())
                .map(|og| OutputEntry {
                    device_id: DeviceId(og.device_id.clone().unwrap_or_default()),
                    mode: og.mode,
                    channels: og.channels.clone(),
                }).collect();

            if let Some(chain_idx) = editing_index {
                if let Some(chain) = session.project.chains.get_mut(chain_idx) {
                    // Find target block: specific index or last OutputBlock
                    let target_idx = io_block_idx.unwrap_or_else(|| {
                        chain.blocks.iter().rposition(|b| matches!(&b.kind, AudioBlockKind::Output(_))).unwrap_or(chain.blocks.len().saturating_sub(1))
                    });
                    if let Some(block) = chain.blocks.get_mut(target_idx) {
                        if let AudioBlockKind::Output(ref mut ob) = block.kind {
                            ob.entries = new_entries;
                        }
                    }
                    if let Err(msg) = chain.validate_channel_conflicts() {
                        groups_window.set_status_message(msg.into());
                        return;
                    }
                    let chain_id = chain.id.clone();
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                        groups_window.set_status_message(error.to_string().into());
                        return;
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                }
            }
            *chain_draft.borrow_mut() = None;
            groups_window.set_status_message("".into());
            let _ = groups_window.hide();
        });
    }
    {
        let weak_groups_window = chain_output_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        chain_output_groups_window.on_cancel(move || {
            let Some(groups_window) = weak_groups_window.upgrade() else {
                return;
            };
            *chain_draft.borrow_mut() = None;
            let _ = groups_window.hide();
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_groups_window = chain_output_groups_window.as_weak();
        chain_output_groups_window.on_toggle_enabled(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(gw) = weak_groups_window.upgrade() else { return; };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else { return; };
            let Some(chain_idx) = draft.editing_index else { return; };
            let Some(block_idx) = draft.editing_io_block_index else { return; };
            drop(draft_borrow);
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            let Some(block) = chain.blocks.get_mut(block_idx) else { return; };
            block.enabled = !block.enabled;
            gw.set_block_enabled(block.enabled);
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("toggle I/O block enabled: {e}");
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_groups_window = chain_output_groups_window.as_weak();
        chain_output_groups_window.on_delete_block(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(gw) = weak_groups_window.upgrade() else { return; };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else { return; };
            let Some(chain_idx) = draft.editing_index else { return; };
            let Some(block_idx) = draft.editing_io_block_index else { return; };
            drop(draft_borrow);
            *chain_draft.borrow_mut() = None;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            if block_idx < chain.blocks.len() {
                chain.blocks.remove(block_idx);
            }
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("delete I/O block: {e}");
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            let _ = gw.hide();
        });
    }
    {
        let weak_main_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_model_options = block_model_options.clone();
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
            block_model_options.set_vec(items);
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
                        let stream_model = model_id.clone();
                        timer.start(
                            slint::TimerMode::Repeated,
                            std::time::Duration::from_millis(50),
                            move || {
                                let Some(win) = weak_win.upgrade() else { return; };
                                let runtime_borrow = runtime.borrow();
                                let kind: slint::SharedString = if stream_model == "spectrum_analyzer" {
                                    "spectrum".into()
                                } else {
                                    "stream".into()
                                };
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
                win.set_block_model_option_labels(ModelRc::from(win_model_labels.clone()));
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
                    let stream_model_id = model_id.clone();
                    let mut poll_count: u32 = 0;
                    stream_timer.start(
                        slint::TimerMode::Repeated,
                        std::time::Duration::from_millis(50),
                        move || {
                            let Some(win) = weak_win_stream.upgrade() else { return; };
                            let runtime_borrow = project_runtime_stream.borrow();
                            let kind: slint::SharedString = if stream_model_id == "spectrum_analyzer" {
                                "spectrum".into()
                            } else {
                                "stream".into()
                            };
                            let Some(runtime) = runtime_borrow.as_ref() else {
                                poll_count += 1;
                                if poll_count % 40 == 0 {
                                    log::warn!("[block-editor-stream] runtime not available (poll #{})", poll_count);
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
                show_child_window(window.window(), win.window());
                open_block_windows.borrow_mut().push(BlockWindow { chain_index: ci, block_index: bi, window: win, stream_timer: block_stream_timer });
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_model_options = block_model_options.clone();
        let block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let multi_slider_points = multi_slider_points.clone();
        let curve_editor_points = curve_editor_points.clone();
        let eq_band_curves = eq_band_curves.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        window.on_clear_chain_block(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = None;
            block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
            multi_slider_points.set_vec(Vec::new());
            curve_editor_points.set_vec(Vec::new());
            eq_band_curves.set_vec(Vec::new());
            window.set_eq_total_curve("".into());
            set_selected_block(&window, None, None);
            window.set_show_block_drawer(false);
            window.set_show_block_type_picker(false);
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                let _ = block_editor_window.hide();
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_toggle_chain_block_enabled(move |chain_index, ui_block_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let Some(chain) = session.project.chains.get_mut(chain_index as usize) else {
                set_status_error(&window, &toast_timer, "Chain inválida.");
                return;
            };
            // Convert UI index to real block index from current chain state
            let block_index = ui_index_to_real_block_index(chain, ui_block_index as usize);
            log::info!("on_toggle_chain_block_enabled: chain_index={}, ui_index={}, real_index={}", chain_index, ui_block_index, block_index);
            let Some(block) = chain.blocks.get_mut(block_index) else {
                set_status_error(&window, &toast_timer, "Block inválido.");
                return;
            };
            block.enabled = !block.enabled;
            let new_enabled = block.enabled;
            let chain_id = chain.id.clone();
            // Keep block_editor_draft in sync to prevent stale persist from reverting
            if let Some(draft) = block_editor_draft.borrow_mut().as_mut() {
                if draft.chain_index == chain_index as usize && draft.block_index == Some(block_index) {
                    draft.enabled = new_enabled;
                }
            }
            // Keep inline drawer UI in sync
            window.set_block_drawer_enabled(new_enabled);
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
            let chain_ref = session.project.chains.get(chain_index as usize);
            *selected_block.borrow_mut() = Some(SelectedBlock {
                chain_index: chain_index as usize,
                block_index,
            });
            set_selected_block(&window, selected_block.borrow().as_ref(), chain_ref);
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            clear_status(&window, &toast_timer);
        });
    }
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        let open_block_windows = open_block_windows.clone();
        window.on_reorder_chain_block(move |chain_index, ui_from_index, ui_before_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let (chain_id, _insert_at) = {
                let Some(chain) = session.project.chains.get_mut(chain_index as usize) else {
                    set_status_error(&window, &toast_timer, "Chain inválida.");
                    return;
                };
                // Both from_index and before_index are in UI space — convert to real indices
                let from_index = ui_index_to_real_block_index(chain, ui_from_index as usize) as i32;
                let real_before = ui_index_to_real_block_index(chain, ui_before_index as usize) as i32;
                log::info!("[reorder_chain_block] chain_index={}, ui_from={} → real_from={}, ui_before={} → real_before={}", chain_index, ui_from_index, from_index, ui_before_index, real_before);
                let block_count = chain.blocks.len() as i32;
                if from_index < 0 || from_index >= block_count {
                    return;
                }
                if real_before == from_index || real_before == from_index + 1 {
                    return;
                }
                let block = chain.blocks.remove(from_index as usize);
                let mut normalized_before = real_before;
                if normalized_before > from_index {
                    normalized_before -= 1;
                }
                let insert_at = normalized_before.clamp(0, chain.blocks.len() as i32) as usize;
                chain.blocks.insert(insert_at, block);
                (chain.id.clone(), insert_at)
            };
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
            // Close editor and clear all state — avoids stale index references
            block_editor_persist_timer.stop();
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = None;
            // Close all open block editor windows for this chain
            {
                let ci = chain_index as usize;
                for bw in open_block_windows.borrow().iter() {
                    if bw.chain_index == ci {
                        let _ = bw.window.hide();
                    }
                }
                open_block_windows.borrow_mut().retain(|bw| bw.chain_index != ci);
            }
            window.set_show_block_drawer(false);
            window.set_show_block_type_picker(false);
            set_selected_block(&window, None, None);
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            clear_status(&window, &toast_timer);
        });
    }
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_type_options = block_type_options.clone();
        let block_model_options = block_model_options.clone();
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
            block_model_options.set_vec(items);
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
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_model_options = block_model_options.clone();
        let block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let multi_slider_points = multi_slider_points.clone();
        let curve_editor_points = curve_editor_points.clone();
        let eq_band_curves = eq_band_curves.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        window.on_cancel_block_picker(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            block_editor_persist_timer.stop();
            *block_editor_draft.borrow_mut() = None;
            block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
            multi_slider_points.set_vec(Vec::new());
            curve_editor_points.set_vec(Vec::new());
            eq_band_curves.set_vec(Vec::new());
            window.set_eq_total_curve("".into());
            window.set_block_drawer_selected_model_index(-1);
            window.set_block_drawer_selected_type_index(-1);
            window.set_show_block_type_picker(false);
            window.set_show_block_drawer(false);
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                let _ = block_editor_window.hide();
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_model_options = block_model_options.clone();
        let block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let multi_slider_points = multi_slider_points.clone();
        let curve_editor_points = curve_editor_points.clone();
        let eq_band_curves = eq_band_curves.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let inline_stream_timer = inline_stream_timer.clone();
        window.on_close_block_drawer(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            block_editor_persist_timer.stop();
            *inline_stream_timer.borrow_mut() = None;
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = None;
            block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
            multi_slider_points.set_vec(Vec::new());
            curve_editor_points.set_vec(Vec::new());
            eq_band_curves.set_vec(Vec::new());
            window.set_eq_total_curve("".into());
            window.set_block_drawer_selected_model_index(-1);
            window.set_block_drawer_selected_type_index(-1);
            set_selected_block(&window, None, None);
            window.set_show_block_type_picker(false);
            window.set_show_block_drawer(false);
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                let _ = block_editor_window.hide();
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_number_text(move |path, value_text| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let normalized = value_text.replace(',', ".");
            let Ok(value) = normalized.parse::<f32>() else {
                log_gui_message("block-drawer.number-text", "Valor numérico inválido.");
                return;
            };
            set_block_parameter_number(&block_parameter_items, path.as_str(), value);
            window.set_block_drawer_status_message("".into());
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
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
                        "block-drawer.number-text",
                    auto_save,
                    );
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_toggle_block_drawer_enabled(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = block_editor_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            draft.enabled = !draft.enabled;
            log::info!("[toggle_block_drawer_enabled] chain_index={}, block_index={:?}, enabled={}, effect_type='{}', model_id='{}'",
                draft.chain_index, draft.block_index, draft.enabled, draft.effect_type, draft.model_id);
            window.set_block_drawer_enabled(draft.enabled);
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_drawer_enabled(draft.enabled);
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
                    "block-drawer.toggle-enabled",
                auto_save,
                );
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_text(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_block_parameter_text(&block_parameter_items, path.as_str(), value.as_str());
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_drawer_status_message("".into());
            }
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
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
                        "block-drawer.text",
                    auto_save,
                    );
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
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
        window.on_update_block_parameter_number(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_block_parameter_number(&block_parameter_items, path.as_str(), value);
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_drawer_status_message("".into());
            }
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
                let params = build_params_from_items(&block_parameter_items);
                let (eq_total, eq_bands) = compute_eq_curves(&draft.effect_type, &draft.model_id, &params);
                eq_band_curves.set_vec(eq_bands.into_iter().map(SharedString::from).collect::<Vec<_>>());
                window.set_eq_total_curve(eq_total.into());
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
                        "block-drawer.number",
                    auto_save,
                    );
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_update_block_parameter_bool(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_block_parameter_bool(&block_parameter_items, path.as_str(), value);
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                block_editor_window.set_block_drawer_status_message("".into());
            }
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
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
                        "block-drawer.bool",
                    auto_save,
                    );
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let select_block_model_options = block_model_options.clone();
        let select_block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_select_block_parameter_option(move |path, index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_block_parameter_option(&block_parameter_items, path.as_str(), index);
            if path.as_str() == SELECT_SELECTED_BLOCK_ID {
                let selected_option_block_id =
                    internal_block_parameter_value(&block_parameter_items, SELECT_SELECTED_BLOCK_ID);
                if let (Some(draft), Some(selected_option_block_id)) = (
                    block_editor_draft.borrow_mut().as_mut(),
                    selected_option_block_id,
                ) {
                    if draft.is_select {
                        if let Some(session) = project_session.borrow().as_ref() {
                            if let Some(block_index) = draft.block_index {
                                if let Some(block) = session
                                    .project
                                    .chains
                                    .get(draft.chain_index)
                                    .and_then(|chain| chain.blocks.get(block_index))
                                {
                                    if let Some(editor_data) = block_editor_data_with_selected(
                                        block,
                                        Some(&selected_option_block_id),
                                    ) {
                                        draft.effect_type = editor_data.effect_type.clone();
                                        draft.model_id = editor_data.model_id.clone();
                                        let items = block_model_picker_items(&editor_data.effect_type, &draft.instrument);
                                        select_block_model_option_labels
                                            .set_vec(block_model_picker_labels(&items));
                                        select_block_model_options.set_vec(items);
                                        block_parameter_items
                                            .set_vec(block_parameter_items_for_editor(&editor_data));
                                        window.set_block_drawer_selected_type_index(block_type_index(
                                            &editor_data.effect_type,
                                            &draft.instrument,
                                        ));
                                        window.set_block_drawer_selected_model_index(
                                            block_model_index(
                                                &editor_data.effect_type,
                                                &editor_data.model_id,
                                                &draft.instrument,
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            window.set_block_drawer_status_message("".into());
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
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
                        "block-drawer.option",
                    auto_save,
                    );
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let block_editor_draft = block_editor_draft.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        window.on_pick_block_parameter_file(move |path| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let extensions = block_parameter_extensions(&block_parameter_items, path.as_str());
            let mut dialog = FileDialog::new();
            if !extensions.is_empty() {
                let refs = extensions
                    .iter()
                    .map(|value| value.as_str())
                    .collect::<Vec<_>>();
                dialog = dialog.add_filter("Arquivos suportados", &refs);
            }
            let Some(file) = dialog.pick_file() else {
                return;
            };
            set_block_parameter_text(
                &block_parameter_items,
                path.as_str(),
                file.to_string_lossy().as_ref(),
            );
            window.set_block_drawer_status_message("".into());
            if let Some(draft) = block_editor_draft.borrow().as_ref() {
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
                        "block-drawer.file",
                    auto_save,
                    );
                }
            }
        });
    }
    {
        let vst3_handles = vst3_editor_handles_for_on_open.clone();
        let vst3_sr = vst3_sample_rate;
        window.on_open_vst3_editor(move |model_id| {
            match project::vst3_editor::open_vst3_editor(model_id.as_str(), vst3_sr) {
                Ok(handle) => { vst3_handles.borrow_mut().push(handle); }
                Err(e) => { log::error!("VST3 editor: failed to open '{}': {}", model_id, e); }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_model_options = block_model_options.clone();
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
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let open_compact_window_save = open_compact_window.clone();
        let project_session_save = project_session.clone();
        window.on_save_block_drawer(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            block_editor_persist_timer.stop();
            let Some(draft) = block_editor_draft.borrow().clone() else {
                return;
            };
            if let Err(error) = persist_block_editor_draft(
                &window,
                &draft,
                &block_parameter_items,
                &project_session,
                &project_chains,
                &project_runtime,
                &saved_project_snapshot,
                &project_dirty,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
                true,
                auto_save,
            ) {
                log::error!("[adapter-gui] block-drawer.save: {error}");
                window.set_block_drawer_status_message(error.to_string().into());
                return;
            }
            *selected_block.borrow_mut() = None;
            set_selected_block(&window, None, None);
            *block_editor_draft.borrow_mut() = None;
            block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
            multi_slider_points.set_vec(Vec::new());
            curve_editor_points.set_vec(Vec::new());
            eq_band_curves.set_vec(Vec::new());
            window.set_eq_total_curve("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                let _ = block_editor_window.hide();
            }
            // Refresh compact chain view if open
            if let Some((ci, weak_cw)) = open_compact_window_save.borrow().as_ref() {
                if let Some(cw) = weak_cw.upgrade() {
                    let session_borrow = project_session_save.borrow();
                    if let Some(session) = session_borrow.as_ref() {
                        let blocks = build_compact_blocks(&session.project, *ci);
                        cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
                    }
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_model_options = block_model_options.clone();
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
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_delete_block_drawer(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            block_editor_persist_timer.stop();
            let Some(draft) = block_editor_draft.borrow().clone() else {
                return;
            };
            let Some(block_index) = draft.block_index else {
                return;
            };
            let confirmed = rfd::MessageDialog::new()
                .set_title("Excluir bloco")
                .set_description(format!("Excluir o bloco \"{}\"?", draft.model_id))
                .set_buttons(rfd::MessageButtons::YesNo)
                .set_level(rfd::MessageLevel::Warning)
                .show();
            if !matches!(confirmed, rfd::MessageDialogResult::Yes) {
                return;
            }
            log::info!("on_delete_block: chain_index={}, block_index={}, effect_type='{}', model_id='{}'", draft.chain_index, block_index, draft.effect_type, draft.model_id);
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                log_gui_message("block-drawer.delete", "Nenhum projeto carregado.");
                return;
            };
            let Some(chain) = session.project.chains.get_mut(draft.chain_index) else {
                log_gui_message("block-drawer.delete", "Chain inválida.");
                return;
            };
            if block_index >= chain.blocks.len() {
                log_gui_message("block-drawer.delete", "Block inválido.");
                return;
            }
            let chain_id = chain.id.clone();
            chain.blocks.remove(block_index);
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("[adapter-gui] block-drawer.delete: {error}");
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = None;
            block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
            multi_slider_points.set_vec(Vec::new());
            curve_editor_points.set_vec(Vec::new());
            eq_band_curves.set_vec(Vec::new());
            window.set_eq_total_curve("".into());
            set_selected_block(&window, None, None);
            window.set_show_block_drawer(false);
            window.set_block_drawer_status_message("".into());
            clear_status(&window, &toast_timer);
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                let _ = block_editor_window.hide();
            }
        });
    }
    // Fullscreen inline chain editor callbacks — delegate to ChainEditorWindow
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_edit_chain_input(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_edit_input(index);
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_remove_chain_input(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_remove_input(index);
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_add_chain_input(move || {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_add_input();
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_edit_chain_output(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_edit_output(index);
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_remove_chain_output(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_remove_output(index);
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_add_chain_output(move || {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_add_output();
            }
        });
    }
    {
        let chain_editor_window = chain_editor_window.clone();
        window.on_select_chain_instrument(move |index| {
            if let Some(cew) = chain_editor_window.borrow().as_ref() {
                cew.invoke_select_instrument(index);
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
    {
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_chain_window = chain_editor_window.clone();
        let weak_input_groups_window = chain_input_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft_for_input_save = io_block_insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        chain_input_window.on_save(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(input_window) = weak_input_window.upgrade() else {
                return;
            };

            // Handle I/O block insert mode: insert a single InputBlock at the stored position
            let io_insert = io_block_insert_draft_for_input_save.borrow().clone();
            log::info!("[input_window.on_save] io_insert={:?}", io_insert.as_ref().map(|d| format!("kind={}, chain={}, before={}", d.kind, d.chain_index, d.before_index)));
            if let Some(io_draft) = io_insert {
                if io_draft.kind == "input" {
                    log::info!("[input_window.on_save] INSERTING NEW InputBlock at chain={}, before={}", io_draft.chain_index, io_draft.before_index);
                    // Extract what we need from chain_draft, then drop the borrow
                    let input_group = {
                        let draft_borrow = chain_draft.borrow();
                        let Some(draft) = draft_borrow.as_ref() else {
                            let _ = input_window.hide();
                            drop(draft_borrow);
                            *io_block_insert_draft_for_input_save.borrow_mut() = None;
                            return;
                        };
                        let Some(ig) = draft.inputs.first().cloned() else {
                            let _ = input_window.hide();
                            drop(draft_borrow);
                            *io_block_insert_draft_for_input_save.borrow_mut() = None;
                            return;
                        };
                        ig
                    };
                    if input_group.device_id.is_none() || input_group.channels.is_empty() {
                        input_window.set_status_message("Selecione dispositivo e canais.".into());
                        return;
                    }
                    let chain_index = io_draft.chain_index;
                    let before_index = io_draft.before_index;
                    // Clear drafts BEFORE touching session to avoid borrow conflicts
                    *io_block_insert_draft_for_input_save.borrow_mut() = None;
                    *chain_draft.borrow_mut() = None;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        let _ = input_window.hide();
                        return;
                    };
                    let Some(chain) = session.project.chains.get_mut(chain_index) else {
                        let _ = input_window.hide();
                        return;
                    };
                    let real_chain_id = chain.id.clone();
                    let input_block = AudioBlock {
                        id: BlockId::generate_for_chain(&real_chain_id),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            entries: vec![InputEntry {
                                device_id: DeviceId(input_group.device_id.clone().unwrap_or_default()),
                                mode: input_group.mode,
                                channels: input_group.channels.clone(),
                            }],
                        }),
                    };
                    let insert_pos = before_index.min(chain.blocks.len());
                    chain.blocks.insert(insert_pos, input_block);
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &real_chain_id) {
                        eprintln!("io block insert error: {error}");
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &*input_chain_devices.borrow(),
                        &*output_chain_devices.borrow(),
                    );
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                    input_window.set_status_message("".into());
                    let _ = input_window.hide();
                    return;
                }
            }

            log::info!("[input_window.on_save] NORMAL FLOW — editing existing entry in InputBlock");
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                let _ = input_window.hide();
                return;
            };
            let Some(gi) = draft.editing_input_index else {
                log::warn!("[input_window.on_save] no editing_input_index set!");
                let _ = input_window.hide();
                return;
            };
            log::info!("[input_window.on_save] editing_input_index={}, draft.inputs.len={}", gi, draft.inputs.len());
            let Some(input_group) = draft.inputs.get(gi) else {
                let _ = input_window.hide();
                return;
            };
            if input_group.device_id.is_none() || input_group.channels.is_empty() {
                input_window.set_status_message("Selecione dispositivo e canais.".into());
                return;
            }
            if let Some(index) = draft.editing_index {
                let mut session_borrow = project_session.borrow_mut();
                let Some(session) = session_borrow.as_mut() else {
                    return;
                };
                let Some(chain) = session.project.chains.get_mut(index) else {
                    return;
                };
                // Rebuild chain blocks: replace all InputBlocks with new ones from draft
                let new_input_blocks: Vec<AudioBlock> = draft.inputs.iter().enumerate().map(|(i, ig)| AudioBlock {
                    id: BlockId(format!("{}:input:{}", chain.id.0, i)),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
                            mode: ig.mode,
                            channels: ig.channels.clone(),
                        }],
                    }),
                }).collect();
                // Keep non-input, non-output blocks and existing output blocks
                let non_input_blocks: Vec<AudioBlock> = chain.blocks.iter()
                    .filter(|b| !matches!(&b.kind, AudioBlockKind::Input(_)))
                    .cloned()
                    .collect();
                let mut all_blocks = Vec::with_capacity(new_input_blocks.len() + non_input_blocks.len());
                all_blocks.extend(new_input_blocks);
                all_blocks.extend(non_input_blocks);
                chain.blocks = all_blocks;
                let chain_id = chain.id.clone();
                if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                    eprintln!("input editor save error: {error}");
                    return;
                }
                replace_project_chains(
                    &project_chains,
                    &session.project,
                    &*input_chain_devices.borrow(),
                    &*output_chain_devices.borrow(),
                );
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            }
            if let Some(chain_window) = weak_chain_window.borrow().as_ref() {
                apply_chain_io_groups(
                    &window,
                    chain_window,
                    draft,
                    &*input_chain_devices.borrow(),
                    &*output_chain_devices.borrow(),
                );
            }
            // Refresh input groups window if open
            if let Some(groups_window) = weak_input_groups_window.upgrade() {
                let (input_items, _) =
                    build_io_group_items(draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                groups_window
                    .set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
            }
            // Clear the adding flag on successful save
            draft.adding_new_input = false;
            input_window.set_status_message("".into());
            let _ = input_window.hide();
        });
    }
    {
        let weak_output_window = chain_output_window.as_weak();
        let chain_editor_window_for_out_cancel = chain_editor_window.clone();
        let weak_window_for_out_cancel = window.as_weak();
        let weak_output_groups_for_cancel = chain_output_groups_window.as_weak();
        let io_block_insert_draft_for_output_cancel = io_block_insert_draft.clone();
        let chain_draft_for_output_cancel = chain_draft.clone();
        let input_chain_devices_for_out_cancel = input_chain_devices.clone();
        let output_chain_devices_for_out_cancel = output_chain_devices.clone();
        chain_output_window.on_cancel(move || {
            if let Some(output_window) = weak_output_window.upgrade() {
                output_window.set_status_message("".into());
                let _ = output_window.hide();
            }
            if io_block_insert_draft_for_output_cancel.borrow().is_some() {
                *io_block_insert_draft_for_output_cancel.borrow_mut() = None;
                *chain_draft_for_output_cancel.borrow_mut() = None;
                return;
            }
            // If we were adding a new entry, remove the placeholder
            let mut draft_borrow = chain_draft_for_output_cancel.borrow_mut();
            if let Some(draft) = draft_borrow.as_mut() {
                if draft.adding_new_output {
                    if let Some(idx) = draft.editing_output_index {
                        if idx < draft.outputs.len() {
                            draft.outputs.remove(idx);
                        }
                    }
                    draft.adding_new_output = false;
                    draft.editing_output_index = None;
                    // Refresh chain editor window
                    if let Some(window) = weak_window_for_out_cancel.upgrade() {
                        if let Some(chain_window) = chain_editor_window_for_out_cancel.borrow().as_ref() {
                            apply_chain_io_groups(
                                &window,
                                chain_window,
                                draft,
                                &*input_chain_devices_for_out_cancel.borrow(),
                                &*output_chain_devices_for_out_cancel.borrow(),
                            );
                        }
                    }
                    // Refresh groups window if open
                    if let Some(groups_window) = weak_output_groups_for_cancel.upgrade() {
                        let (_, output_items) =
                            build_io_group_items(draft, &*input_chain_devices_for_out_cancel.borrow(), &*output_chain_devices_for_out_cancel.borrow());
                        groups_window
                            .set_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
                    }
                }
            }
        });
    }
    {
        let weak_input_window = chain_input_window.as_weak();
        let chain_editor_window_for_cancel = chain_editor_window.clone();
        let weak_window_for_cancel = window.as_weak();
        let weak_input_groups_for_cancel = chain_input_groups_window.as_weak();
        let io_block_insert_draft_for_input_cancel = io_block_insert_draft.clone();
        let chain_draft_for_input_cancel = chain_draft.clone();
        let input_chain_devices_for_cancel = input_chain_devices.clone();
        let output_chain_devices_for_cancel = output_chain_devices.clone();
        chain_input_window.on_cancel(move || {
            if let Some(input_window) = weak_input_window.upgrade() {
                input_window.set_status_message("".into());
                let _ = input_window.hide();
            }
            if io_block_insert_draft_for_input_cancel.borrow().is_some() {
                *io_block_insert_draft_for_input_cancel.borrow_mut() = None;
                *chain_draft_for_input_cancel.borrow_mut() = None;
                return;
            }
            // If we were adding a new entry, remove the placeholder
            let mut draft_borrow = chain_draft_for_input_cancel.borrow_mut();
            if let Some(draft) = draft_borrow.as_mut() {
                if draft.adding_new_input {
                    if let Some(idx) = draft.editing_input_index {
                        if idx < draft.inputs.len() {
                            draft.inputs.remove(idx);
                        }
                    }
                    draft.adding_new_input = false;
                    draft.editing_input_index = None;
                    // Refresh chain editor window
                    if let Some(window) = weak_window_for_cancel.upgrade() {
                        if let Some(chain_window) = chain_editor_window_for_cancel.borrow().as_ref() {
                            apply_chain_io_groups(
                                &window,
                                chain_window,
                                draft,
                                &*input_chain_devices_for_cancel.borrow(),
                                &*output_chain_devices_for_cancel.borrow(),
                            );
                        }
                    }
                    // Refresh groups window if open
                    if let Some(groups_window) = weak_input_groups_for_cancel.upgrade() {
                        let (input_items, _) =
                            build_io_group_items(draft, &*input_chain_devices_for_cancel.borrow(), &*output_chain_devices_for_cancel.borrow());
                        groups_window
                            .set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
                    }
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let chain_editor_window_ref = chain_editor_window.clone();
        let weak_output_groups_window = chain_output_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft_for_output_save = io_block_insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        chain_output_window.on_save(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(output_window) = weak_output_window.upgrade() else {
                return;
            };

            // Handle I/O block insert mode: insert a single OutputBlock at the stored position
            let io_insert = io_block_insert_draft_for_output_save.borrow().clone();
            if let Some(io_draft) = io_insert {
                if io_draft.kind == "output" {
                    let output_group = {
                        let draft_borrow = chain_draft.borrow();
                        let Some(draft) = draft_borrow.as_ref() else {
                            let _ = output_window.hide();
                            drop(draft_borrow);
                            *io_block_insert_draft_for_output_save.borrow_mut() = None;
                            return;
                        };
                        let Some(og) = draft.outputs.first().cloned() else {
                            let _ = output_window.hide();
                            drop(draft_borrow);
                            *io_block_insert_draft_for_output_save.borrow_mut() = None;
                            return;
                        };
                        og
                    };
                    if output_group.device_id.is_none() || output_group.channels.is_empty() {
                        output_window.set_status_message("Selecione dispositivo e canais.".into());
                        return;
                    }
                    let chain_index = io_draft.chain_index;
                    let before_index = io_draft.before_index;
                    *io_block_insert_draft_for_output_save.borrow_mut() = None;
                    *chain_draft.borrow_mut() = None;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        let _ = output_window.hide();
                        return;
                    };
                    let Some(chain) = session.project.chains.get_mut(chain_index) else {
                        let _ = output_window.hide();
                        return;
                    };
                    let real_chain_id = chain.id.clone();
                    let output_block = AudioBlock {
                        id: BlockId::generate_for_chain(&real_chain_id),
                        enabled: true,
                        kind: AudioBlockKind::Output(OutputBlock {
                            model: "standard".to_string(),
                            entries: vec![OutputEntry {
                                device_id: DeviceId(output_group.device_id.clone().unwrap_or_default()),
                                mode: output_group.mode,
                                channels: output_group.channels.clone(),
                            }],
                        }),
                    };
                    let insert_pos = before_index.min(chain.blocks.len());
                    chain.blocks.insert(insert_pos, output_block);
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &real_chain_id) {
                        eprintln!("io block insert error: {error}");
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &*input_chain_devices.borrow(),
                        &*output_chain_devices.borrow(),
                    );
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                    output_window.set_status_message("".into());
                    let _ = output_window.hide();
                    return;
                }
            }

            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                let _ = output_window.hide();
                return;
            };
            let Some(gi) = draft.editing_output_index else {
                let _ = output_window.hide();
                return;
            };
            let Some(output_group) = draft.outputs.get(gi) else {
                let _ = output_window.hide();
                return;
            };
            if output_group.device_id.is_none() || output_group.channels.is_empty() {
                output_window.set_status_message("Selecione dispositivo e canais.".into());
                return;
            }
            if let Some(index) = draft.editing_index {
                let mut session_borrow = project_session.borrow_mut();
                let Some(session) = session_borrow.as_mut() else {
                    return;
                };
                let Some(chain) = session.project.chains.get_mut(index) else {
                    return;
                };
                // Rebuild chain blocks: replace all OutputBlocks with new ones from draft
                let new_output_blocks: Vec<AudioBlock> = draft.outputs.iter().enumerate().map(|(i, og)| AudioBlock {
                    id: BlockId(format!("{}:output:{}", chain.id.0, i)),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId(og.device_id.clone().unwrap_or_default()),
                            mode: og.mode,
                            channels: og.channels.clone(),
                        }],
                    }),
                }).collect();
                // Keep non-output blocks (inputs and audio blocks)
                let non_output_blocks: Vec<AudioBlock> = chain.blocks.iter()
                    .filter(|b| !matches!(&b.kind, AudioBlockKind::Output(_)))
                    .cloned()
                    .collect();
                let mut all_blocks = Vec::with_capacity(non_output_blocks.len() + new_output_blocks.len());
                all_blocks.extend(non_output_blocks);
                all_blocks.extend(new_output_blocks);
                chain.blocks = all_blocks;
                let chain_id = chain.id.clone();
                if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                    eprintln!("output editor save error: {error}");
                    return;
                }
                replace_project_chains(
                    &project_chains,
                    &session.project,
                    &*input_chain_devices.borrow(),
                    &*output_chain_devices.borrow(),
                );
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            }
            if let Some(chain_window) = chain_editor_window_ref.borrow().as_ref() {
                apply_chain_io_groups(
                    &window,
                    chain_window,
                    draft,
                    &*input_chain_devices.borrow(),
                    &*output_chain_devices.borrow(),
                );
            }
            // Refresh output groups window if open
            if let Some(groups_window) = weak_output_groups_window.upgrade() {
                let (_, output_items) =
                    build_io_group_items(draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                groups_window
                    .set_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
            }
            // Clear the adding flag on successful save
            draft.adding_new_output = false;
            output_window.set_status_message("".into());
            let _ = output_window.hide();
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_remove_chain(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let chain_name = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                    return;
                };
                let index = index as usize;
                if index >= session.project.chains.len() {
                    set_status_error(&window, &toast_timer, "Chain inválida.");
                    return;
                }
                session.project.chains[index]
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("Chain {}", index + 1))
            };
            let confirmed = rfd::MessageDialog::new()
                .set_title("Excluir chain")
                .set_description(format!("Excluir a chain \"{}\"?", chain_name))
                .set_buttons(rfd::MessageButtons::YesNo)
                .set_level(rfd::MessageLevel::Warning)
                .show();
            if !matches!(confirmed, rfd::MessageDialogResult::Yes) {
                return;
            }
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let index = index as usize;
            if index >= session.project.chains.len() {
                return;
            }
            let removed_chain_id = session.project.chains[index].id.clone();
            session.project.chains.remove(index);
            remove_live_chain_runtime(&project_runtime, &removed_chain_id);
            replace_project_chains(
                &project_chains,
                &session.project,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            clear_status(&window, &toast_timer);
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_toggle_chain_enabled(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let index = index as usize;
            let Some(chain) = session.project.chains.get(index) else {
                set_status_error(&window, &toast_timer, "Chain inválida.");
                return;
            };
            let will_enable = !chain.enabled;
            log::info!("on_toggle_chain_enabled: index={}, will_enable={}", index, will_enable);
            // Check channel conflict before enabling
            if will_enable {
                let chain_id = chain.id.clone();
                let our_inputs = chain.input_blocks();
                let mut conflict = false;
                'outer: for other in &session.project.chains {
                    if other.id != chain_id && other.enabled {
                        for (_, other_input) in other.input_blocks() {
                            for (_, our_input) in &our_inputs {
                                let other_entries_conflict = other_input.entries.iter().any(|oe|
                                    our_input.entries.iter().any(|ue|
                                        oe.device_id == ue.device_id
                                        && oe.channels.iter().any(|ch| ue.channels.contains(ch))
                                    )
                                );
                                if other_entries_conflict
                                {
                                    let other_name = other.description.as_deref().unwrap_or("outra chain");
                                    set_status_error(&window, &toast_timer, &format!("Input channel já em uso por '{}'", other_name));
                                    conflict = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
                if conflict {
                    return;
                }
            }
            let Some(chain) = session.project.chains.get_mut(index) else { return; };
            chain.enabled = will_enable;
            let chain_id = chain.id.clone();
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
            // enabled is runtime-only state — do NOT mark project as dirty
            clear_status(&window, &toast_timer);
        });
    }
    // Ao fechar a janela principal, encerra todo o processo
    window.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });

    // Latency polling timer — reads measured latency from runtime and updates chain items
    let latency_timer = Timer::default();
    {
        let weak_window = window.as_weak();
        let project_runtime_lat = project_runtime.clone();
        let project_session_lat = project_session.clone();
        let project_chains_lat = project_chains.clone();
        latency_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(500),
            move || {
                let Some(_win) = weak_window.upgrade() else { return; };
                let session_borrow = project_session_lat.borrow();
                let Some(session) = session_borrow.as_ref() else { return; };
                let rt_borrow = project_runtime_lat.borrow();
                let Some(rt) = rt_borrow.as_ref() else { return; };
                for (i, chain) in session.project.chains.iter().enumerate() {
                    if let Some(measured) = rt.measured_latency_ms(&chain.id) {
                        if measured > 0.1 {
                            if let Some(mut item) = project_chains_lat.row_data(i) {
                                if (item.latency_ms - measured).abs() > 0.5 {
                                    item.latency_ms = measured;
                                    project_chains_lat.set_row_data(i, item);
                                }
                            }
                        }
                    }
                }
            },
        );
    }

    window.run().map_err(|error| anyhow!(error.to_string()))
}
fn resolve_project_paths() -> ProjectPaths {
    ProjectPaths {
        default_config_path: parse_path_argument("--config").unwrap_or_else(|| {
            let local = PathBuf::from("config.yaml");
            if local.exists() {
                local
            } else {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config.yaml")
            }
        }),
    }
}
fn load_and_sync_app_config() -> Result<AppConfig> {
    let mut config = FilesystemStorage::load_app_config().unwrap_or_default();
    let changed = sync_recent_projects(&mut config);
    if changed {
        let _ = FilesystemStorage::save_app_config(&config);
    }
    Ok(config)
}
fn sync_recent_projects(config: &mut AppConfig) -> bool {
    let original = config.clone();
    let mut synced = Vec::new();
    for recent in &config.recent_projects {
        let path = PathBuf::from(&recent.project_path);
        // Skip path.exists() check here — it can block indefinitely on
        // disconnected network volumes or external drives (macOS stat hang).
        // Validity is checked lazily when the user tries to open the project.
        let canonical_path = if path.is_absolute() {
            path.clone()
        } else {
            env::current_dir().map(|d| d.join(&path)).unwrap_or(path.clone())
        };
        let canonical_path_string = canonical_path.to_string_lossy().to_string();
        if synced
            .iter()
            .any(|current: &RecentProjectEntry| current.project_path == canonical_path_string)
        {
            continue;
        }
        synced.push(RecentProjectEntry {
            project_path: canonical_path_string,
            project_name: if recent.project_name.trim().is_empty() {
                UNTITLED_PROJECT_NAME.to_string()
            } else {
                recent.project_name.clone()
            },
            is_valid: true,
            invalid_reason: None,
        });
    }
    config.recent_projects = synced;
    *config != original
}
fn canonical_project_path(path: &PathBuf) -> Result<PathBuf> {
    // Do NOT call path.exists() here — blocks on disconnected network volumes.
    // fs::canonicalize resolves symlinks and normalises the path without blocking
    // for paths that exist on local storage; for paths that don't exist it errors
    // and we fall back to the raw path.
    if let Ok(c) = fs::canonicalize(path) {
        return Ok(c);
    }
    if path.is_absolute() {
        return Ok(path.clone());
    }
    Ok(env::current_dir()?.join(path))
}
fn register_recent_project(config: &mut AppConfig, path: &PathBuf, name: &str) {
    let canonical_path = canonical_project_path(path).unwrap_or(path.clone());
    let path_string = canonical_path.to_string_lossy().to_string();
    config
        .recent_projects
        .retain(|current| current.project_path != path_string);
    config.recent_projects.insert(
        0,
        RecentProjectEntry {
            project_path: path_string,
            project_name: if name.trim().is_empty() {
                UNTITLED_PROJECT_NAME.to_string()
            } else {
                name.trim().to_string()
            },
            is_valid: true,
            invalid_reason: None,
        },
    );
}
fn mark_recent_project_invalid(config: &mut AppConfig, path: &PathBuf, reason: &str) {
    let canonical_path = canonical_project_path(path).unwrap_or(path.clone());
    let path_string = canonical_path.to_string_lossy().to_string();
    if let Some(recent) = config
        .recent_projects
        .iter_mut()
        .find(|current| current.project_path == path_string)
    {
        recent.is_valid = false;
        recent.invalid_reason = Some(if reason.trim().is_empty() {
            "Projeto inválido".to_string()
        } else {
            reason.trim().to_string()
        });
    }
}
fn recent_project_items(
    recent_projects: &[RecentProjectEntry],
    query: &str,
) -> Vec<RecentProjectItem> {
    let query = query.trim().to_lowercase();
    recent_projects
        .iter()
        .enumerate()
        .filter(|(_, recent)| {
            if query.is_empty() {
                return true;
            }
            recent.project_name.to_lowercase().contains(&query)
                || recent.project_path.to_lowercase().contains(&query)
        })
        .map(|(original_index, recent)| RecentProjectItem {
            original_index: original_index as i32,
            title: if recent.project_name.trim().is_empty() {
                UNTITLED_PROJECT_NAME.into()
            } else {
                recent.project_name.clone().into()
            },
            subtitle: recent.project_path.clone().into(),
            is_valid: recent.is_valid,
            invalid_reason: recent.invalid_reason.clone().unwrap_or_default().into(),
        })
        .collect()
}
fn project_display_name(project: &Project) -> String {
    project
        .name
        .as_ref()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| UNTITLED_PROJECT_NAME.to_string())
}
fn parse_path_argument(flag: &str) -> Option<PathBuf> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next().map(PathBuf::from);
        }
    }
    None
}
fn create_new_project_session(default_config_path: &Path) -> ProjectSession {
    let config = if default_config_path.exists() {
        load_app_config(default_config_path).unwrap_or_default()
    } else {
        AppConfigYaml {
            presets_path: Some(PathBuf::from("./presets")),
        }
    };
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
    };
    ProjectSession {
        project,
        project_path: None,
        config_path: None,
        presets_path: config
            .presets_path
            .unwrap_or_else(|| PathBuf::from("./presets")),
    }
}
fn load_app_config(path: &Path) -> Result<AppConfigYaml> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
}
fn resolve_project_config_path(project_path: &Path) -> PathBuf {
    project_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("config.yaml")
}
fn load_project_session(project_path: &Path, config_path: &Path) -> Result<ProjectSession> {
    log::info!("loading project session from {:?}", project_path);
    let config = if config_path.exists() {
        load_app_config(config_path)?
    } else {
        AppConfigYaml::default()
    };
    let presets_path = config
        .presets_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("./presets"));
    let mut project = YamlProjectRepository {
        path: project_path.to_path_buf(),
    }
    .load_current_project()?;

    // Populate device_settings from per-machine config (gui-settings.yaml)
    // instead of the project YAML. Old projects may still have device_settings
    // in their YAML — those are read for backward compat but overridden here.
    let gui_settings = FilesystemStorage::load_gui_audio_settings()
        .ok()
        .flatten()
        .unwrap_or_default();
    project.device_settings = gui_settings
        .input_devices
        .iter()
        .chain(gui_settings.output_devices.iter())
        .map(|g| DeviceSettings {
            device_id: DeviceId(g.device_id.clone()),
            sample_rate: g.sample_rate,
            buffer_size_frames: g.buffer_size_frames,
            bit_depth: g.bit_depth,
        })
        .collect();

    Ok(ProjectSession {
        project,
        project_path: Some(project_path.to_path_buf()),
        config_path: Some(config_path.to_path_buf()),
        presets_path: project_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(presets_path),
    })
}
fn replace_project_chains(
    model: &Rc<VecModel<ProjectChainItem>>,
    project: &Project,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) {
    let items = project
        .chains
        .iter()
        .enumerate()
        .map(|(index, chain)| {
            let input_settings = chain.first_input()
                .and_then(|ib| ib.entries.first())
                .and_then(|entry| {
                    project.device_settings.iter().find(|s| s.device_id == entry.device_id)
                });
            let output_settings = chain.last_output()
                .and_then(|ob| ob.entries.first())
                .and_then(|entry| {
                    project.device_settings.iter().find(|s| s.device_id == entry.device_id)
                });
            let input_buffer =
                input_settings.map(|s| s.buffer_size_frames).unwrap_or(256) as f32;
            let output_buffer =
                output_settings.map(|s| s.buffer_size_frames).unwrap_or(256) as f32;
            let sample_rate = input_settings.map(|s| s.sample_rate).unwrap_or(48000) as f32;
            let latency_ms = (input_buffer + output_buffer) / sample_rate * 1000.0;
            ProjectChainItem {
                instrument: chain.instrument.clone().into(),
                title: chain
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("Chain {}", index + 1))
                    .into(),
                subtitle: chain_routing_summary(chain).into(),
                enabled: chain.enabled,
                block_count_label: {
                    let effect_block_count = chain.blocks.iter()
                        .filter(|b| !matches!(&b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
                        .count();
                    if effect_block_count == 1 {
                        "1 block".into()
                    } else {
                        format!("{} blocks", effect_block_count).into()
                    }
                },
                input_label: {
                    let input_chs: Vec<usize> = chain.input_blocks().into_iter()
                        .flat_map(|(_, ib)| ib.entries.iter().flat_map(|e| e.channels.iter().copied()))
                        .collect();
                    chain_endpoint_label("In", &input_chs).into()
                },
                input_tooltip: chain_inputs_tooltip(chain, project, input_devices).into(),
                output_label: {
                    let output_chs: Vec<usize> = chain.output_blocks().into_iter()
                        .flat_map(|(_, ob)| ob.entries.iter().flat_map(|e| e.channels.iter().copied()))
                        .collect();
                    chain_endpoint_label("Out", &output_chs).into()
                },
                output_tooltip: chain_outputs_tooltip(chain, project, output_devices)
                .into(),
                latency_ms,
                blocks: {
                    let first_input_idx = chain.blocks.iter().position(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
                    let last_output_idx = chain.blocks.iter().rposition(|b| matches!(&b.kind, AudioBlockKind::Output(_)));
                    log::info!("[replace_project_chains] chain[{}] '{}' UI blocks:", index, chain.description.as_deref().unwrap_or(""));
                    for (real_idx, b) in chain.blocks.iter().enumerate() {
                        if Some(real_idx) == first_input_idx || Some(real_idx) == last_output_idx {
                            continue;
                        }
                        log::info!("[replace_project_chains]   real_index={} kind={}", real_idx, b.model_ref().map(|m| format!("{}/{}", m.effect_type, m.model)).unwrap_or_else(|| "io/insert".to_string()));
                    }
                    ModelRc::from(Rc::new(VecModel::from(
                        chain
                            .blocks
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| {
                                // Hide only the first Input (fixed chip) and last Output (fixed chip)
                                Some(*i) != first_input_idx && Some(*i) != last_output_idx
                            })
                            .map(|(real_idx, b)| {
                                let mut item = chain_block_item_from_block(b);
                                item.real_index = real_idx as i32;
                                item
                            })
                            .collect::<Vec<_>>(),
                    )))
                },
            }
        })
        .collect::<Vec<_>>();
    model.set_vec(items);
}
fn chain_endpoint_label(prefix: &str, _channels: &[usize]) -> String {
    prefix.to_string()
}
fn chain_inputs_tooltip(
    chain: &Chain,
    _project: &Project,
    devices: &[AudioDeviceDescriptor],
) -> String {
    // Show only entries from the FIRST InputBlock (chip In)
    let first_input = chain.first_input();
    let Some(input) = first_input else {
        return "No input configured".to_string();
    };
    input.entries.iter().enumerate().map(|(ei, entry)| {
            let device_name = devices
                .iter()
                .find(|d| d.id == entry.device_id.0)
                .map(|d| d.name.as_str())
                .unwrap_or(&entry.device_id.0);
            let mode = match entry.mode {
                ChainInputMode::Mono => "Mono",
                ChainInputMode::Stereo => "Stereo",
                ChainInputMode::DualMono => "Dual Mono",
            };
            let label = format!("Input #{}", ei + 1);
            format!("{}: {} · {} · Ch {}", label, device_name, mode, format_channel_list(&entry.channels))
    }).collect::<Vec<_>>().join("\n")
}
fn chain_outputs_tooltip(
    chain: &Chain,
    _project: &Project,
    devices: &[AudioDeviceDescriptor],
) -> String {
    // Show only entries from the LAST OutputBlock (chip Out)
    let last_output = chain.last_output();
    let Some(output) = last_output else {
        return "No output configured".to_string();
    };
    output.entries.iter().enumerate().map(|(ei, entry)| {
        let device_name = devices
            .iter()
            .find(|d| d.id == entry.device_id.0)
            .map(|d| d.name.as_str())
            .unwrap_or(&entry.device_id.0);
        let mode = match entry.mode {
            ChainOutputMode::Mono => "Mono",
            ChainOutputMode::Stereo => "Stereo",
        };
        let label = format!("Output #{}", ei + 1);
        format!("{}: {} · {} · Ch {}", label, device_name, mode, format_channel_list(&entry.channels))
    }).collect::<Vec<_>>().join("\n")
}
fn format_channel_list(channels: &[usize]) -> String {
    if channels.is_empty() {
        "-".to_string()
    } else {
        channels
            .iter()
            .map(|channel| (channel + 1).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}
fn block_type_picker_items(instrument: &str) -> Vec<BlockTypePickerItem> {
    let mut seen = std::collections::BTreeSet::new();
    let mut items: Vec<BlockTypePickerItem> = supported_block_types()
        .into_iter()
        .filter(|item| seen.insert(item.effect_type))
        .map(|item| BlockTypePickerItem {
            effect_type: item.effect_type.into(),
            label: item.display_label.into(),
            subtitle: "".into(),
            icon_kind: item.icon_kind.into(),
            use_panel_editor: item.use_panel_editor,
            accent_color: crate::ui_state::accent_color_for_icon_kind(item.icon_kind),
            icon_source: slint::Image::default(),
        })
        .filter(|item| {
            instrument == block_core::INST_GENERIC || !block_model_picker_items(item.effect_type.as_str(), instrument).is_empty()
        })
        .collect();
    // Add I/O block types
    items.push(BlockTypePickerItem {
        effect_type: "input".into(),
        label: "INPUT".into(),
        subtitle: "".into(),
        icon_kind: "input".into(),
        use_panel_editor: false,
        accent_color: crate::ui_state::accent_color_for_icon_kind("routing"),
        icon_source: slint::Image::default(),
    });
    items.push(BlockTypePickerItem {
        effect_type: "output".into(),
        label: "OUTPUT".into(),
        subtitle: "".into(),
        icon_kind: "output".into(),
        use_panel_editor: false,
        accent_color: crate::ui_state::accent_color_for_icon_kind("routing"),
        icon_source: slint::Image::default(),
    });
    items.push(BlockTypePickerItem {
        effect_type: "insert".into(),
        label: "INSERT".into(),
        subtitle: "".into(),
        icon_kind: "insert".into(),
        use_panel_editor: false,
        accent_color: crate::ui_state::accent_color_for_icon_kind("insert"),
        icon_source: slint::Image::default(),
    });
    items
}
fn block_model_picker_items(effect_type: &str, instrument: &str) -> Vec<BlockModelPickerItem> {
    let all_models = supported_block_models(effect_type).unwrap_or_default();
    log::trace!("[block_model_picker_items] effect_type='{}', instrument='{}', total_models={}", effect_type, instrument, all_models.len());
    all_models
        .into_iter()
        .filter(|item| instrument == block_core::INST_GENERIC || item.supported_instruments.iter().any(|i| i == instrument))
        .map(|item| {
            let brand = &item.brand;
            let label = if brand.is_empty() || brand == block_core::BRAND_NATIVE {
                item.display_name.clone()
            } else {
                let brand_display = block_core::capitalize_first(brand);
                format!("{} {}", brand_display, item.display_name)
            };
            let visual = visual_config::visual_config_for_model(&item.brand, &item.model_id);
            let [r, g, b] = visual.panel_bg;
            let panel_bg = slint::Color::from_argb_u8(0xff, r, g, b);
            let [r, g, b] = visual.panel_text;
            let panel_text = slint::Color::from_argb_u8(0xff, r, g, b);
            let [r, g, b] = visual.brand_strip_bg;
            let brand_strip_bg = slint::Color::from_argb_u8(0xff, r, g, b);
            BlockModelPickerItem {
                effect_type: item.effect_type.clone().into(),
                model_id: item.model_id.clone().into(),
                label: label.into(),
                display_name: item.display_name.clone().into(),
                subtitle: "".into(),
                icon_kind: supported_block_type(effect_type)
                    .map(|entry| entry.icon_kind)
                    .unwrap_or(effect_type)
                    .into(),
                brand: item.brand.clone().into(),
                type_label: item.type_label.clone().into(),
                panel_bg,
                panel_text,
                brand_strip_bg,
                model_font: visual.model_font.into(),
                photo_offset_x: visual.photo_offset_x,
                photo_offset_y: visual.photo_offset_y,
            }
        })
        .collect()
}
fn block_model_picker_labels(items: &[BlockModelPickerItem]) -> Vec<SharedString> {
    items.iter().map(|item| item.label.clone()).collect()
}
fn set_selected_block(window: &AppWindow, selected_block: Option<&SelectedBlock>, chain: Option<&Chain>) {
    if let Some(selected_block) = selected_block {
        let ui_index = chain
            .and_then(|c| real_block_index_to_ui(c, selected_block.block_index))
            .map(|i| i as i32)
            .unwrap_or(selected_block.block_index as i32);
        window.set_selected_chain_block_chain_index(selected_block.chain_index as i32);
        window.set_selected_chain_block_index(ui_index);
    } else {
        window.set_selected_chain_block_chain_index(-1);
        window.set_selected_chain_block_index(-1);
    }
}
fn block_type_index(effect_type: &str, instrument: &str) -> i32 {
    block_type_picker_items(instrument)
        .into_iter()
        .position(|item| item.effect_type.as_str() == effect_type)
        .map(|index| index as i32)
        .unwrap_or(-1)
}
fn block_model_index_from_items(items: &VecModel<BlockModelPickerItem>, model_id: &str) -> i32 {
    for i in 0..items.row_count() {
        if let Some(item) = items.row_data(i) {
            if item.model_id.as_str() == model_id {
                return i as i32;
            }
        }
    }
    0
}
fn block_model_index(effect_type: &str, model_id: &str, instrument: &str) -> i32 {
    supported_block_models(effect_type)
        .unwrap_or_default()
        .into_iter()
        .filter(|item| instrument == block_core::INST_GENERIC || item.supported_instruments.iter().any(|i| i == instrument))
        .position(|item| item.model_id == model_id)
        .map(|index| index as i32)
        .unwrap_or(-1)
}
fn unit_label(unit: &ParameterUnit) -> &'static str {
    match unit {
        ParameterUnit::None => "",
        ParameterUnit::Decibels => "dB",
        ParameterUnit::Hertz => "Hz",
        ParameterUnit::Milliseconds => "ms",
        ParameterUnit::Percent => "%",
        ParameterUnit::Ratio => "Ratio",
        ParameterUnit::Semitones => "st",
    }
}
fn build_compact_blocks(
    project: &Project,
    chain_index: usize,
) -> Vec<CompactBlockItem> {
    let Some(chain) = project.chains.get(chain_index) else {
        return Vec::new();
    };
    chain
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(block_index, block)| {
            let editor_data = block_editor_data(block)?;
            let effect_type = editor_data.effect_type.clone();
            let model_id = editor_data.model_id.clone();
            let params = block_parameter_items_for_editor(&editor_data);
            let knob_layout =
                project::catalog::model_knob_layout(&effect_type, &model_id);
            let overlays = build_knob_overlays(knob_layout, &params);
            let icon_kind = supported_block_type(&effect_type)
                .map(|t| t.icon_kind.to_string())
                .unwrap_or_default();
            let visual = project::catalog::supported_block_models(&effect_type)
                .ok()
                .and_then(|models| {
                    models
                        .into_iter()
                        .find(|m| m.model_id == model_id)
                });

            Some(CompactBlockItem {
                chain_index: chain_index as i32,
                block_index: block_index as i32,
                block_id: block.id.0.clone().into(),
                effect_type: effect_type.clone().into(),
                model_id: model_id.clone().into(),
                icon_kind: icon_kind.clone().into(),
                brand: visual
                    .as_ref()
                    .map(|v| v.brand.clone())
                    .unwrap_or_default()
                    .into(),
                display_name: visual
                    .as_ref()
                    .map(|v| v.display_name.clone())
                    .unwrap_or_default()
                    .into(),
                type_label: visual
                    .as_ref()
                    .map(|v| v.type_label.clone())
                    .unwrap_or_default()
                    .into(),
                enabled: block.enabled,
                panel_bg: {
                    let brand_str = visual.as_ref().map(|v| v.brand.as_str()).unwrap_or("");
                    let vc = visual_config::visual_config_for_model(brand_str, &model_id);
                    let [r, g, b] = vc.panel_bg;
                    slint::Color::from_argb_u8(0xff, r, g, b)
                },
                panel_text: {
                    let brand_str = visual.as_ref().map(|v| v.brand.as_str()).unwrap_or("");
                    let vc = visual_config::visual_config_for_model(brand_str, &model_id);
                    let [r, g, b] = vc.panel_text;
                    slint::Color::from_argb_u8(0xff, r, g, b)
                },
                accent_color: crate::ui_state::accent_color_for_icon_kind(&icon_kind),
                display_label: {
                    let bt = supported_block_type(&effect_type);
                    bt.map(|e| e.display_label).unwrap_or("BLOCK").into()
                },
                icon_source: slint::Image::default(),
                knob_overlays: ModelRc::from(Rc::new(VecModel::from(overlays))),
                parameter_items: ModelRc::from(Rc::new(VecModel::from(params))),
                model_labels: {
                    let instrument = chain.instrument.as_str();
                    let items = block_model_picker_items(&effect_type, instrument);
                    let labels: Vec<SharedString> = items.iter().map(|i| i.label.clone()).collect();
                    ModelRc::from(Rc::new(VecModel::from(labels)))
                },
                model_selected_index: {
                    let instrument = chain.instrument.as_str();
                    let items = block_model_picker_items(&effect_type, instrument);
                    items.iter().position(|i| i.model_id.as_str() == model_id).map(|i| i as i32).unwrap_or(-1)
                },
                stream_data: Default::default(),
                has_external_gui: project::catalog::block_has_external_gui(&effect_type),
            })
        })
        .collect()
}

fn block_editor_data(block: &AudioBlock) -> Option<BlockEditorData> {
    block_editor_data_with_selected(block, None)
}
fn block_editor_data_with_selected(
    block: &AudioBlock,
    selected_option_block_id: Option<&str>,
) -> Option<BlockEditorData> {
    match &block.kind {
        AudioBlockKind::Select(select) => {
            let selected = selected_option_block_id
                .and_then(|selected_id| {
                    select
                        .options
                        .iter()
                        .find(|option| option.id.0 == selected_id)
                })
                .or_else(|| select.selected_option())?;
            let model = selected.model_ref()?;
            Some(BlockEditorData {
                effect_type: model.effect_type.to_string(),
                model_id: model.model.to_string(),
                params: model.params.clone(),
                enabled: block.enabled,
                is_select: true,
                select_options: select
                    .options
                    .iter()
                    .filter_map(|option| {
                        let model = option.model_ref()?;
                        let label = schema_for_block_model(model.effect_type, model.model)
                            .map(|schema| schema.display_name)
                            .unwrap_or_else(|_| model.model.to_string());
                        Some(SelectOptionEditorItem {
                            block_id: option.id.0.clone(),
                            label,
                        })
                    })
                    .collect(),
                selected_select_option_block_id: Some(select.selected_block_id.0.clone()),
            })
        }
        _ => block.model_ref().map(|model| BlockEditorData {
            effect_type: model.effect_type.to_string(),
            model_id: model.model.to_string(),
            params: model.params.clone(),
            enabled: block.enabled,
            is_select: false,
            select_options: Vec::new(),
            selected_select_option_block_id: None,
        }),
    }
}
fn block_parameter_items_for_editor(data: &BlockEditorData) -> Vec<BlockParameterItem> {
    let mut items = Vec::new();
    if !data.select_options.is_empty() {
        let option_labels = data
            .select_options
            .iter()
            .map(|option| SharedString::from(option.label.as_str()))
            .collect::<Vec<_>>();
        let option_values = data
            .select_options
            .iter()
            .map(|option| SharedString::from(option.block_id.as_str()))
            .collect::<Vec<_>>();
        let selected_option_index = data
            .selected_select_option_block_id
            .as_ref()
            .and_then(|selected| {
                data.select_options
                    .iter()
                    .position(|option| &option.block_id == selected)
            })
            .map(|index| index as i32)
            .unwrap_or(0);
        items.push(BlockParameterItem {
            path: SELECT_SELECTED_BLOCK_ID.into(),
            label: "Modelo ativo".into(),
            group: "Select".into(),
            widget_kind: "enum".into(),
            unit_text: "".into(),
            value_text: data
                .selected_select_option_block_id
                .clone()
                .unwrap_or_default()
                .into(),
            numeric_value: 0.0,
            numeric_min: 0.0,
            numeric_max: 1.0,
            numeric_step: 0.0,
            numeric_integer: false,
            bool_value: false,
            selected_option_index,
            option_labels: ModelRc::from(Rc::new(VecModel::from(option_labels))),
            option_values: ModelRc::from(Rc::new(VecModel::from(option_values))),
            file_extensions: ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
            optional: false,
            allow_empty: false,
        });
    }
    items.extend(block_parameter_items_for_model(
        &data.effect_type,
        &data.model_id,
        &data.params,
    ));
    items
}
fn block_parameter_items_for_model(
    effect_type: &str,
    model_id: &str,
    params: &ParameterSet,
) -> Vec<BlockParameterItem> {
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return Vec::new();
    };
    schema
        .parameters
        .iter()
        .filter(|spec| spec.path != "enabled")
        .map(|spec| {
            let current = params
                .get(&spec.path)
                .cloned()
                .or_else(|| spec.default_value.clone())
                .unwrap_or(domain::value_objects::ParameterValue::Null);
            let (numeric_value, numeric_min, numeric_max, numeric_step) = match &spec.domain {
                ParameterDomain::IntRange { min, max, .. } => (
                    current.as_i64().unwrap_or(*min) as f32,
                    *min as f32,
                    *max as f32,
                    match &spec.domain {
                        ParameterDomain::IntRange { step, .. } => *step as f32,
                        _ => 1.0,
                    },
                ),
                ParameterDomain::FloatRange { min, max, .. } => (
                    current.as_f32().unwrap_or(*min),
                    *min,
                    *max,
                    match &spec.domain {
                        ParameterDomain::FloatRange { step, .. } => *step,
                        _ => 0.0,
                    },
                ),
                _ => (0.0, 0.0, 1.0, 0.0),
            };
            let (option_labels, option_values, selected_option_index, file_extensions) = match &spec
                .domain
            {
                ParameterDomain::Enum { options } => {
                    let labels = options
                        .iter()
                        .map(|option| SharedString::from(option.label.as_str()))
                        .collect::<Vec<_>>();
                    let values = options
                        .iter()
                        .map(|option| SharedString::from(option.value.as_str()))
                        .collect::<Vec<_>>();
                    let selected = current
                        .as_str()
                        .and_then(|value| options.iter().position(|option| option.value == value))
                        .map(|index| index as i32)
                        .unwrap_or(0);
                    (
                        ModelRc::from(Rc::new(VecModel::from(labels))),
                        ModelRc::from(Rc::new(VecModel::from(values))),
                        selected,
                        ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                    )
                }
                ParameterDomain::FilePath { extensions } => {
                    let values = extensions
                        .iter()
                        .map(|value| SharedString::from(value.as_str()))
                        .collect::<Vec<_>>();
                    (
                        ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                        ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                        -1,
                        ModelRc::from(Rc::new(VecModel::from(values))),
                    )
                }
                _ => (
                    ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                    ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                    -1,
                    ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                ),
            };
            BlockParameterItem {
                path: spec.path.clone().into(),
                label: spec.label.to_uppercase().into(),
                group: spec.group.clone().unwrap_or_default().into(),
                widget_kind: match &spec.widget {
                    ParameterWidget::MultiSlider | ParameterWidget::CurveEditor { .. } => "",
                    _ => match &spec.domain {
                        ParameterDomain::Bool => "bool",
                        ParameterDomain::IntRange { min, max, step } => {
                            numeric_widget_kind(*min as f32, *max as f32, *step as f32, true)
                        }
                        ParameterDomain::FloatRange { min, max, step } => {
                            numeric_widget_kind(*min, *max, *step, false)
                        }
                        ParameterDomain::Enum { .. } => "enum",
                        ParameterDomain::Text => "text",
                        ParameterDomain::FilePath { .. } => "path",
                    },
                }
                .into(),
                unit_text: unit_label(&spec.unit).into(),
                value_text: match current {
                    domain::value_objects::ParameterValue::String(ref value) => {
                        value.clone().into()
                    }
                    domain::value_objects::ParameterValue::Int(value) => value.to_string().into(),
                    domain::value_objects::ParameterValue::Float(value) => {
                        format!("{value:.2}").into()
                    }
                    domain::value_objects::ParameterValue::Bool(value) => {
                        if value {
                            "true".into()
                        } else {
                            "false".into()
                        }
                    }
                    domain::value_objects::ParameterValue::Null => "".into(),
                },
                numeric_value,
                numeric_min,
                numeric_max,
                numeric_step,
                numeric_integer: matches!(&spec.domain, ParameterDomain::IntRange { .. }),
                bool_value: current.as_bool().unwrap_or(false),
                selected_option_index,
                option_labels,
                option_values,
                file_extensions,
                optional: spec.optional,
                allow_empty: spec.allow_empty,
            }
        })
        .collect()
}
fn set_block_parameter_text(model: &Rc<VecModel<BlockParameterItem>>, path: &str, value: &str) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                row.value_text = value.into();
                model.set_row_data(index, row);
                break;
            }
        }
    }
}
fn set_block_parameter_bool(model: &Rc<VecModel<BlockParameterItem>>, path: &str, value: bool) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                row.bool_value = value;
                model.set_row_data(index, row);
                break;
            }
        }
    }
}
fn set_block_parameter_number(model: &Rc<VecModel<BlockParameterItem>>, path: &str, value: f32) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                let quantized = quantize_numeric_value(
                    value,
                    row.numeric_min,
                    row.numeric_max,
                    row.numeric_step,
                    row.numeric_integer,
                );
                row.numeric_value = quantized;
                row.value_text = if row.numeric_integer {
                    format!("{:.0}", quantized.round()).into()
                } else {
                    format!("{quantized:.2}").into()
                };
                model.set_row_data(index, row);
                break;
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn persist_block_editor_draft(
    window: &AppWindow,
    draft: &BlockEditorDraft,
    block_parameter_items: &Rc<VecModel<BlockParameterItem>>,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    saved_project_snapshot: &Rc<RefCell<Option<String>>>,
    project_dirty: &Rc<RefCell<bool>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
    close_after_save: bool,
    auto_save: bool,
) -> Result<()> {
    let params =
        block_parameter_values(block_parameter_items, &draft.effect_type, &draft.model_id)?;
    log::info!("[persist] effect_type='{}', model_id='{}', close_after_save={}, params:", draft.effect_type, draft.model_id, close_after_save);
    for (path, value) in params.values.iter() {
        log::info!("[persist]   {} = {:?}", path, value);
    }
    let selected_select_option_block_id = if draft.is_select {
        Some(
            internal_block_parameter_value(block_parameter_items, SELECT_SELECTED_BLOCK_ID)
                .ok_or_else(|| anyhow!("Seleção do select inválida."))?,
        )
    } else {
        None
    };
    let mut session_borrow = project_session.borrow_mut();
    let session = session_borrow
        .as_mut()
        .ok_or_else(|| anyhow!("Nenhum projeto carregado."))?;
    let chain_id = {
        let chain = session
            .project
            .chains
            .get_mut(draft.chain_index)
            .ok_or_else(|| anyhow!("Chain inválida."))?;
        if let Some(block_index) = draft.block_index {
            let block = chain
                .blocks
                .get_mut(block_index)
                .ok_or_else(|| anyhow!("Block inválido."))?;
            block.enabled = draft.enabled;
            if draft.is_select {
                let AudioBlockKind::Select(select) = &mut block.kind else {
                    return Err(anyhow!("Block selecionado não é um select."));
                };
                let selected_option_block_id = selected_select_option_block_id
                    .as_ref()
                    .expect("select option id should exist");
                let select_family = select
                    .options
                    .iter()
                    .find_map(|option| option.model_ref().map(|model| model.effect_type.to_string()))
                    .ok_or_else(|| anyhow!("Select sem opções válidas."))?;
                if select_family != draft.effect_type {
                    return Err(anyhow!(
                        "Select só aceita opções do tipo '{}'.",
                        select_family
                    ));
                }
                select.selected_block_id = BlockId(selected_option_block_id.clone());
                let option = select
                    .options
                    .iter_mut()
                    .find(|option| option.id.0 == *selected_option_block_id)
                    .ok_or_else(|| anyhow!("Opção ativa do select não existe."))?;
                let option_id = option.id.clone();
                let option_enabled = option.enabled;
                option.kind = build_audio_block_kind(&draft.effect_type, &draft.model_id, params)
                    .map_err(|error| anyhow!(error))?;
                option.id = option_id;
                option.enabled = option_enabled;
            } else {
                let block_id = block.id.clone();
                block.kind = build_audio_block_kind(&draft.effect_type, &draft.model_id, params)
                    .map_err(|error| anyhow!(error))?;
                block.id = block_id;
            }
        } else {
            let kind = build_audio_block_kind(&draft.effect_type, &draft.model_id, params)
                .map_err(|error| anyhow!(error))?;
            let insert_index = draft.before_index.min(chain.blocks.len());
            log::info!("[persist] INSERT new block at index={}, effect_type='{}', model_id='{}'", insert_index, draft.effect_type, draft.model_id);
            chain.blocks.insert(
                insert_index,
                AudioBlock {
                    id: BlockId::generate_for_chain(&chain.id),
                    enabled: draft.enabled,
                    kind,
                },
            );
            log::info!("[persist] chain after insert has {} blocks:", chain.blocks.len());
            for (i, b) in chain.blocks.iter().enumerate() {
                log::info!("[persist]   [{}] id='{}' kind={}", i, b.id.0, b.model_ref().map(|m| format!("{}/{}", m.effect_type, m.model)).unwrap_or_else(|| "io/insert".to_string()));
            }
        }
        chain.id.clone()
    };
    if let Err(error) = sync_live_chain_runtime(project_runtime, session, &chain_id) {
        log_gui_error("block-drawer.persist", &error);
        return Err(error);
    }
    replace_project_chains(
        project_chains,
        &session.project,
        input_chain_devices,
        output_chain_devices,
    );
    sync_project_dirty(window, session, saved_project_snapshot, project_dirty, auto_save);
    if close_after_save {
        window.set_show_block_drawer(false);
        window.set_show_block_type_picker(false);
        window.set_block_drawer_selected_model_index(-1);
        window.set_block_drawer_selected_type_index(-1);
    }
    window.set_block_drawer_status_message("".into());
    window.set_status_message("".into());
    Ok(())
}
fn quantize_numeric_value(value: f32, min: f32, max: f32, step: f32, integer: bool) -> f32 {
    let mut clamped = value.clamp(min, max);
    if step > 0.0 {
        let snapped_steps = ((clamped - min) / step).round();
        clamped = min + (snapped_steps * step);
        clamped = clamped.clamp(min, max);
    }
    if integer {
        clamped.round()
    } else {
        clamped
    }
}
fn numeric_widget_kind(min: f32, max: f32, step: f32, integer: bool) -> &'static str {
    if step > 0.0 && max > min {
        let steps = ((max - min) / step).round();
        if steps <= 24.0 {
            return "stepper";
        }
    }
    let _ = integer;
    "slider"
}
const BAND_COLORS: &[slint::Color] = &[
    slint::Color::from_argb_u8(255, 232, 77, 77),    // red
    slint::Color::from_argb_u8(255, 77, 184, 232),   // cyan
    slint::Color::from_argb_u8(255, 119, 232, 77),   // green
    slint::Color::from_argb_u8(255, 232, 184, 77),   // orange
    slint::Color::from_argb_u8(255, 184, 77, 232),   // purple
    slint::Color::from_argb_u8(255, 77, 232, 184),   // teal
    slint::Color::from_argb_u8(255, 232, 77, 184),   // pink
    slint::Color::from_argb_u8(255, 184, 232, 77),   // lime
];

fn build_multi_slider_points(
    effect_type: &str,
    model_id: &str,
    params: &ParameterSet,
) -> Vec<MultiSliderPoint> {
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return Vec::new();
    };
    schema
        .parameters
        .iter()
        .filter(|spec| matches!(spec.widget, ParameterWidget::MultiSlider))
        .map(|spec| {
            let current = params
                .get(&spec.path)
                .and_then(|v| v.as_f32())
                .or_else(|| spec.default_value.as_ref().and_then(|v| v.as_f32()))
                .unwrap_or(0.0);
            let (min, max, step) = match &spec.domain {
                ParameterDomain::FloatRange { min, max, step } => (*min, *max, *step),
                _ => (0.0, 1.0, 0.0),
            };
            MultiSliderPoint {
                path: spec.path.clone().into(),
                label: spec.label.clone().into(),
                value: current,
                min_val: min,
                max_val: max,
                step,
            }
        })
        .collect()
}

fn build_curve_editor_points(
    effect_type: &str,
    model_id: &str,
    params: &ParameterSet,
) -> Vec<CurveEditorPoint> {
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return Vec::new();
    };

    // Group CurveEditor params by group name
    let mut groups: Vec<String> = Vec::new();
    for spec in &schema.parameters {
        if let ParameterWidget::CurveEditor { .. } = &spec.widget {
            let group = spec.group.clone().unwrap_or_default();
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
    }

    groups
        .iter()
        .enumerate()
        .map(|(i, group)| {
            let band_color = BAND_COLORS[i % BAND_COLORS.len()];
            let mut point = CurveEditorPoint {
                group: group.clone().into(),
                band_color,
                y_path: "".into(),
                y_value: 0.0,
                y_min: 0.0,
                y_max: 0.0,
                y_step: 0.0,
                y_label: "".into(),
                has_x: false,
                x_path: "".into(),
                x_value: 0.0,
                x_min: 0.0,
                x_max: 0.0,
                x_step: 0.0,
                x_label: "".into(),
                has_width: false,
                width_path: "".into(),
                width_value: 0.0,
                width_min: 0.0,
                width_max: 0.0,
                width_step: 0.0,
            };

            for spec in &schema.parameters {
                let spec_group = spec.group.as_deref().unwrap_or("");
                if spec_group != group {
                    continue;
                }
                let ParameterWidget::CurveEditor { role } = &spec.widget else {
                    continue;
                };
                let current = params
                    .get(&spec.path)
                    .and_then(|v| v.as_f32())
                    .or_else(|| spec.default_value.as_ref().and_then(|v| v.as_f32()))
                    .unwrap_or(0.0);
                let (min, max, step) = match &spec.domain {
                    ParameterDomain::FloatRange { min, max, step } => (*min, *max, *step),
                    _ => (0.0, 1.0, 0.0),
                };
                match role {
                    CurveEditorRole::Y => {
                        point.y_path = spec.path.clone().into();
                        point.y_value = current;
                        point.y_min = min;
                        point.y_max = max;
                        point.y_step = step;
                    }
                    CurveEditorRole::X => {
                        point.has_x = true;
                        point.x_path = spec.path.clone().into();
                        point.x_value = current;
                        point.x_min = min;
                        point.x_max = max;
                        point.x_step = step;
                    }
                    CurveEditorRole::Width => {
                        point.has_width = true;
                        point.width_path = spec.path.clone().into();
                        point.width_value = current;
                        point.width_min = min;
                        point.width_max = max;
                        point.width_step = step;
                    }
                }
            }
            // Compute display labels
            point.y_label = if point.y_value >= 0.0 {
                format!("+{:.1}", point.y_value).into()
            } else {
                format!("{:.1}", point.y_value).into()
            };
            point.x_label = if point.has_x {
                if point.x_value >= 1000.0 {
                    format!("{:.1}k", point.x_value / 1000.0).into()
                } else {
                    format!("{}Hz", point.x_value as i32).into()
                }
            } else {
                "".into()
            };
            point
        })
        .collect()
}

fn build_params_from_items(items: &Rc<VecModel<BlockParameterItem>>) -> ParameterSet {
    let mut params = ParameterSet::default();
    for i in 0..items.row_count() {
        if let Some(item) = items.row_data(i) {
            if !item.path.is_empty() {
                params.insert(
                    item.path.to_string(),
                    domain::value_objects::ParameterValue::Float(item.numeric_value),
                );
            }
        }
    }
    params
}

/// Number of frequency points for EQ curve rendering (20Hz–20kHz).
const EQ_CURVE_POINTS: usize = 200;
/// Sample rate assumed for EQ visualization.
const EQ_VIZ_SAMPLE_RATE: f32 = 48_000.0;
/// SVG viewbox width (must match Slint CurveEditorControl viewbox).
const EQ_SVG_W: f32 = 1000.0;
/// SVG viewbox height.
const EQ_SVG_H: f32 = 200.0;
/// Frequency range.
const EQ_FREQ_MIN: f32 = 20.0;
const EQ_FREQ_MAX: f32 = 20_000.0;
/// Gain range in dB (symmetric around 0).
const EQ_GAIN_MIN: f32 = -24.0;
const EQ_GAIN_MAX: f32 = 24.0;

fn freq_to_x(freq: f32) -> f32 {
    let norm = (freq / EQ_FREQ_MIN).log(EQ_FREQ_MAX / EQ_FREQ_MIN);
    (norm.clamp(0.0, 1.0) * EQ_SVG_W).round()
}

fn gain_to_y(gain_db: f32) -> f32 {
    let norm = 1.0 - (gain_db - EQ_GAIN_MIN) / (EQ_GAIN_MAX - EQ_GAIN_MIN);
    (norm.clamp(0.0, 1.0) * EQ_SVG_H).round()
}

fn db_to_linear(db: f32) -> f32 { 10.0_f32.powf(db / 20.0) }
fn linear_to_db(lin: f32) -> f32 { 20.0 * lin.max(1e-10).log10() }

fn biquad_kind_for_group(group: &str) -> block_core::BiquadKind {
    let lower = group.to_lowercase();
    if lower.contains("low") {
        block_core::BiquadKind::LowShelf
    } else if lower.contains("high") {
        block_core::BiquadKind::HighShelf
    } else {
        block_core::BiquadKind::Peak
    }
}

/// Log-spaced frequency points for the curve.
fn eq_frequencies() -> Vec<f32> {
    (0..EQ_CURVE_POINTS)
        .map(|i| {
            let t = i as f32 / (EQ_CURVE_POINTS - 1) as f32;
            EQ_FREQ_MIN * (EQ_FREQ_MAX / EQ_FREQ_MIN).powf(t)
        })
        .collect()
}

fn db_vec_to_svg_path(dbs: &[f32]) -> String {
    let freqs = eq_frequencies();
    let mut path = String::with_capacity(dbs.len() * 12);
    for (i, (&db, &freq)) in dbs.iter().zip(freqs.iter()).enumerate() {
        let x = freq_to_x(freq);
        let y = gain_to_y(db);
        if i == 0 {
            path.push_str(&format!("M {x} {y}"));
        } else {
            path.push_str(&format!(" L {x} {y}"));
        }
    }
    path
}

/// Compute band and total SVG path strings for CurveEditor EQ blocks.
/// Returns (total_curve, band_curves).
fn compute_eq_curves(
    effect_type: &str,
    model_id: &str,
    params: &ParameterSet,
) -> (String, Vec<String>) {
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return (String::new(), Vec::new());
    };

    // Collect groups in order
    let mut groups: Vec<String> = Vec::new();
    for spec in &schema.parameters {
        if let ParameterWidget::CurveEditor { .. } = &spec.widget {
            let group = spec.group.clone().unwrap_or_default();
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
    }
    if groups.is_empty() {
        return (String::new(), Vec::new());
    }

    let freqs = eq_frequencies();
    let mut total_linear = vec![1.0_f32; EQ_CURVE_POINTS];
    let mut band_paths = Vec::with_capacity(groups.len());

    for group in &groups {
        // Extract Y (gain), X (freq), Width (Q) for this group
        let mut gain_db = 0.0_f32;
        let mut freq_hz = 1000.0_f32;
        let mut q = 1.0_f32;

        for spec in &schema.parameters {
            if spec.group.as_deref().unwrap_or("") != group {
                continue;
            }
            let ParameterWidget::CurveEditor { role } = &spec.widget else { continue };
            let val = params
                .get(&spec.path)
                .and_then(|v| v.as_f32())
                .or_else(|| spec.default_value.as_ref().and_then(|v| v.as_f32()))
                .unwrap_or(0.0);
            match role {
                CurveEditorRole::Y => gain_db = val,
                CurveEditorRole::X => freq_hz = val,
                CurveEditorRole::Width => q = val,
            }
        }

        let kind = biquad_kind_for_group(group);
        let filter = block_core::BiquadFilter::new(kind, freq_hz, gain_db, q.max(0.01), EQ_VIZ_SAMPLE_RATE);

        let band_dbs: Vec<f32> = freqs.iter()
            .map(|&f| filter.magnitude_db(f, EQ_VIZ_SAMPLE_RATE))
            .collect();

        // Accumulate linear magnitudes for total curve
        for (lin, &db) in total_linear.iter_mut().zip(band_dbs.iter()) {
            *lin *= db_to_linear(db);
        }

        band_paths.push(db_vec_to_svg_path(&band_dbs));
    }

    let total_dbs: Vec<f32> = total_linear.iter().map(|&lin| linear_to_db(lin)).collect();
    let total_path = db_vec_to_svg_path(&total_dbs);

    (total_path, band_paths)
}

fn set_block_parameter_option(
    model: &Rc<VecModel<BlockParameterItem>>,
    path: &str,
    selected_index: i32,
) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                row.selected_option_index = selected_index;
                if selected_index >= 0 {
                    if let Some(value) = row.option_values.row_data(selected_index as usize) {
                        row.value_text = value;
                    }
                }
                model.set_row_data(index, row);
                break;
            }
        }
    }
}
fn block_parameter_extensions(model: &Rc<VecModel<BlockParameterItem>>, path: &str) -> Vec<String> {
    for index in 0..model.row_count() {
        if let Some(row) = model.row_data(index) {
            if row.path.as_str() == path {
                let mut values = Vec::new();
                for ext_index in 0..row.file_extensions.row_count() {
                    if let Some(ext) = row.file_extensions.row_data(ext_index) {
                        values.push(ext.to_string());
                    }
                }
                return values;
            }
        }
    }
    Vec::new()
}
fn block_parameter_values(
    model: &Rc<VecModel<BlockParameterItem>>,
    effect_type: &str,
    model_id: &str,
) -> Result<ParameterSet> {
    let schema = schema_for_block_model(effect_type, model_id).map_err(|error| anyhow!(error))?;
    let mut params = ParameterSet::default();
    for index in 0..model.row_count() {
        let Some(row) = model.row_data(index) else {
            continue;
        };
        if row.path.as_str().starts_with(SELECT_PATH_PREFIX) {
            continue;
        }
        let value = match row.widget_kind.as_str() {
            "bool" => domain::value_objects::ParameterValue::Bool(row.bool_value),
            "int" => domain::value_objects::ParameterValue::Int(row.numeric_value.round() as i64),
            "float" => domain::value_objects::ParameterValue::Float(row.numeric_value),
            "slider" => {
                if row.numeric_integer {
                    domain::value_objects::ParameterValue::Int(row.numeric_value.round() as i64)
                } else {
                    domain::value_objects::ParameterValue::Float(row.numeric_value)
                }
            }
            "stepper" => {
                if row.numeric_integer {
                    domain::value_objects::ParameterValue::Int(row.numeric_value.round() as i64)
                } else {
                    domain::value_objects::ParameterValue::Float(row.numeric_value)
                }
            }
            "enum" => {
                if row.selected_option_index < 0 {
                    return Err(anyhow!("Selecione uma opção para {}", row.label));
                }
                let selected = row
                    .option_values
                    .row_data(row.selected_option_index as usize)
                    .ok_or_else(|| anyhow!("Seleção inválida para {}", row.label))?;
                domain::value_objects::ParameterValue::String(selected.to_string())
            }
            "text" | "path" => {
                let value = row.value_text.to_string();
                if row.optional && value.trim().is_empty() {
                    domain::value_objects::ParameterValue::Null
                } else {
                    domain::value_objects::ParameterValue::String(value)
                }
            }
            // CurveEditor / MultiSlider params use widget_kind="" and store their
            // value in numeric_value — persist as Float.
            "" => domain::value_objects::ParameterValue::Float(row.numeric_value),
            _ => domain::value_objects::ParameterValue::Null,
        };
        params.insert(row.path.as_str(), value);
    }
    params
        .normalized_against(&schema)
        .map_err(|error| anyhow!(error))
}
fn internal_block_parameter_value(
    model: &Rc<VecModel<BlockParameterItem>>,
    path: &str,
) -> Option<String> {
    for index in 0..model.row_count() {
        let Some(row) = model.row_data(index) else {
            continue;
        };
        if row.path.as_str() != path {
            continue;
        }
        if row.selected_option_index >= 0 {
            if let Some(value) = row.option_values.row_data(row.selected_option_index as usize) {
                return Some(value.to_string());
            }
        }
        return Some(row.value_text.to_string());
    }
    None
}
fn build_project_device_rows(
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
    device_settings: &[DeviceSettings],
) -> Vec<DeviceSelectionItem> {
    let mut rows: Vec<DeviceSelectionItem> = Vec::new();
    for device in input_devices.iter().chain(output_devices.iter()) {
        if rows.iter().any(|row| {
            row.device_id.as_str() == device.id.as_str()
                || row.name.as_str() == device.name.as_str()
        }) {
            continue;
        }
        let config = device_settings
            .iter()
            .find(|setting| setting.device_id.0 == device.id)
            .map(|setting| GuiAudioDeviceSettings {
                device_id: setting.device_id.0.clone(),
                name: device.name.clone(),
                sample_rate: setting.sample_rate,
                buffer_size_frames: setting.buffer_size_frames,
                bit_depth: setting.bit_depth,
            })
            .unwrap_or_else(|| default_device_settings(device.id.clone(), device.name.clone()));
        rows.push(DeviceSelectionItem {
            device_id: config.device_id.into(),
            name: config.name.into(),
            selected: device_settings
                .iter()
                .any(|setting| setting.device_id.0 == device.id),
            sample_rate_text: config.sample_rate.to_string().into(),
            buffer_size_text: config.buffer_size_frames.to_string().into(),
            bit_depth_text: config.bit_depth.to_string().into(),
        });
    }
    rows
}
fn project_session_snapshot(session: &ProjectSession) -> Result<String> {
    infra_yaml::serialize_project(&session.project)
}
fn set_project_dirty(window: &AppWindow, project_dirty: &Rc<RefCell<bool>>, dirty: bool) {
    *project_dirty.borrow_mut() = dirty;
    window.set_project_dirty(dirty);
}
#[track_caller]
fn sync_project_dirty(
    window: &AppWindow,
    session: &ProjectSession,
    saved_project_snapshot: &Rc<RefCell<Option<String>>>,
    project_dirty: &Rc<RefCell<bool>>,
    auto_save: bool,
) {
    // Snapshot audio hardware + JACK state for every UI-triggered mutation so we
    // can correlate a specific user action (knob, device pick, chain edit) with
    // downstream Scarlett/xHCI disconnects in the journal. The caller location
    // identifies which UI callback fired this mutation.
    let caller = std::panic::Location::caller();
    infra_cpal::log_audio_status(&format!(
        "sync_project_dirty from {}:{}",
        caller.file(),
        caller.line()
    ));

    if auto_save {
        if let Some(ref path) = session.project_path {
            match save_project_session(session, path) {
                Ok(()) => {
                    *saved_project_snapshot.borrow_mut() = project_session_snapshot(session).ok();
                    set_project_dirty(window, project_dirty, false);
                    log::debug!("auto-save: saved to {:?}", path);
                    return;
                }
                Err(e) => log::error!("auto-save failed: {e}"),
            }
        }
    }
    let dirty = match saved_project_snapshot.borrow().as_ref() {
        Some(saved_snapshot) => project_session_snapshot(session)
            .map(|current| current != *saved_snapshot)
            .unwrap_or(true),
        None => true,
    };
    set_project_dirty(window, project_dirty, dirty);
}
fn save_project_session(session: &ProjectSession, project_path: &PathBuf) -> Result<()> {
    log::info!("saving project session to {:?}", project_path);
    let parent_dir = project_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent_dir)?;
    fs::write(project_path, project_session_snapshot(session)?)?;
    let config_path = session
        .config_path
        .clone()
        .unwrap_or_else(|| resolve_project_config_path(project_path));
    let config = ConfigYaml {
        presets_path: "./presets".to_string(),
    };
    fs::write(config_path, serde_yaml::to_string(&config)?)?;
    fs::create_dir_all(parent_dir.join("presets"))?;
    Ok(())
}
fn save_chain_blocks_to_preset(chain: &Chain, path: &Path) -> Result<()> {
    let preset = ChainBlocksPreset {
        id: preset_id_from_path(path)?,
        name: chain.description.clone(),
        blocks: chain.blocks.clone(),
    };
    save_chain_preset_file(path, &preset)
}
fn load_preset_file(path: &Path) -> Result<ChainBlocksPreset> {
    load_chain_preset_file(path)
}
fn preset_id_from_path(path: &Path) -> Result<String> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow!("arquivo de preset inválido"))
}
fn project_title_for_path(project_path: Option<&PathBuf>, project: &Project) -> String {
    if let Some(name) = project
        .name
        .as_ref()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
    {
        return name.to_string();
    }
    project_path
        .and_then(|path| path.file_stem())
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| {
            if project.chains.is_empty() {
                "Novo Projeto".to_string()
            } else {
                "Projeto".to_string()
            }
        })
}
fn selected_device_index(devices: &[AudioDeviceDescriptor], selected_id: Option<&str>) -> i32 {
    selected_id
        .and_then(|selected_id| devices.iter().position(|device| device.id == selected_id))
        .map(|index| index as i32)
        .unwrap_or(-1)
}
fn create_chain_draft(
    project: &Project,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) -> ChainDraft {
    let default_input = InputGroupDraft {
        device_id: input_devices.first().map(|device| device.id.clone()),
        channels: Vec::new(),
        mode: ChainInputMode::Mono,
    };
    let default_output = OutputGroupDraft {
        device_id: output_devices.first().map(|device| device.id.clone()),
        channels: Vec::new(),
        mode: ChainOutputMode::Stereo,
    };
    ChainDraft {
        editing_index: None,
        name: format!("Chain {}", project.chains.len() + 1),
        instrument: block_core::DEFAULT_INSTRUMENT.to_string(),
        inputs: vec![default_input],
        outputs: vec![default_output],
        editing_io_block_index: None,
        editing_input_index: None,
        editing_output_index: None,
        adding_new_input: false,
        adding_new_output: false,
    }
}
fn chain_draft_from_chain(index: usize, chain: &Chain) -> ChainDraft {
    // Only show the first InputBlock (fixed, position 0) in the chain editor
    let first_input = chain.input_blocks().into_iter().next();
    let inputs: Vec<InputGroupDraft> = match first_input {
        Some((_, input)) => input
            .entries
            .iter()
            .map(|entry| InputGroupDraft {
                device_id: if entry.device_id.0.is_empty() { None } else { Some(entry.device_id.0.clone()) },
                channels: entry.channels.clone(),
                mode: entry.mode,
            })
            .collect(),
        None => vec![InputGroupDraft {
            device_id: None,
            channels: Vec::new(),
            mode: ChainInputMode::Mono,
        }],
    };
    // Only show the last OutputBlock (fixed, last position) in the chain editor
    let last_output = chain.output_blocks().into_iter().last();
    let outputs: Vec<OutputGroupDraft> = match last_output {
        Some((_, output)) => output
            .entries
            .iter()
            .map(|entry| OutputGroupDraft {
                device_id: if entry.device_id.0.is_empty() { None } else { Some(entry.device_id.0.clone()) },
                channels: entry.channels.clone(),
                mode: entry.mode,
            })
            .collect(),
        None => vec![OutputGroupDraft {
            device_id: None,
            channels: Vec::new(),
            mode: ChainOutputMode::Stereo,
        }],
    };
    ChainDraft {
        editing_index: Some(index),
        name: chain
            .description
            .clone()
            .unwrap_or_else(|| format!("Chain {}", index + 1)),
        instrument: chain.instrument.clone(),
        inputs,
        editing_io_block_index: None,
        outputs,
        editing_input_index: None,
        editing_output_index: None,
        adding_new_input: false,
        adding_new_output: false,
    }
}
fn load_thumbnail_image(effect_type: &str, model_id: &str) -> (slint::Image, bool, f32, f32) {
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        static CACHE: RefCell<HashMap<(String, String), (slint::Image, f32, f32)>> = RefCell::new(HashMap::new());
    }

    let key = (effect_type.to_string(), model_id.to_string());

    let cached = CACHE.with(|c| c.borrow().get(&key).cloned());
    if let Some((img, w, h)) = cached {
        return (img, true, w, h);
    }

    match thumbnails::thumbnail_png(effect_type, model_id) {
        Some(png_bytes) => {
            match image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let w = rgba.width() as f32;
                    let h = rgba.height() as f32;
                    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                        rgba.as_raw(),
                        rgba.width(),
                        rgba.height(),
                    );
                    let slint_img = slint::Image::from_rgba8(buffer);
                    CACHE.with(|c| c.borrow_mut().insert(key, (slint_img.clone(), w, h)));
                    (slint_img, true, w, h)
                }
                Err(e) => {
                    log::warn!("Failed to decode thumbnail for {}/{}: {}", effect_type, model_id, e);
                    (slint::Image::default(), false, 0.0, 0.0)
                }
            }
        }
        None => (slint::Image::default(), false, 0.0, 0.0)
    }
}

fn load_screenshot_image(effect_type: &str, model_id: &str) -> (slint::Image, bool) {
    match plugin_info::screenshot_png(effect_type, model_id) {
        Some(png_bytes) => {
            match image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                        rgba.as_raw(),
                        rgba.width(),
                        rgba.height(),
                    );
                    (slint::Image::from_rgba8(buffer), true)
                }
                Err(e) => {
                    log::warn!(
                        "Failed to decode screenshot for {}/{}: {}",
                        effect_type,
                        model_id,
                        e
                    );
                    (slint::Image::default(), false)
                }
            }
        }
        None => (slint::Image::default(), false),
    }
}

fn system_language() -> String {
    let lang = std::env::var("LANG").unwrap_or_default();
    let base = lang.split('.').next().unwrap_or("");
    // "C", "POSIX", empty, or too short = not a real locale → fall back to English
    if base.is_empty() || base.len() < 2 || matches!(base, "C" | "POSIX") {
        return "en-US".to_string();
    }
    base.replace('_', "-")
}

/// Map a UI block index (which excludes hidden first Input and last Output) to the real chain.blocks index.
fn ui_index_to_real_block_index(chain: &Chain, ui_index: usize) -> usize {
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

/// Map a real chain.blocks index to the UI block index (which excludes hidden first Input and last Output).
fn real_block_index_to_ui(chain: &Chain, real_index: usize) -> Option<usize> {
    let first_input_idx = chain.blocks.iter().position(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
    let last_output_idx = chain.blocks.iter().rposition(|b| matches!(&b.kind, AudioBlockKind::Output(_)));
    let mut visible_count = 0;
    for (idx, _) in chain.blocks.iter().enumerate() {
        if Some(idx) == first_input_idx || Some(idx) == last_output_idx {
            continue;
        }
        if idx == real_index {
            return Some(visible_count);
        }
        visible_count += 1;
    }
    None
}

fn chain_block_item_from_block(block: &AudioBlock) -> ChainBlockItem {
    let (kind, label) = match &block.kind {
        AudioBlockKind::Input(_) => ("input".to_string(), "input".to_string()),
        AudioBlockKind::Output(_) => ("output".to_string(), "output".to_string()),
        AudioBlockKind::Insert(_) => ("insert".to_string(), "insert".to_string()),
        AudioBlockKind::Select(select) => select
            .selected_option()
            .and_then(|option| option.model_ref())
            .map(|model| (model.effect_type.to_string(), model.model.to_string()))
            .unwrap_or_else(|| ("select".to_string(), "select".to_string())),
        _ => block
            .model_ref()
            .map(|block| (block.effect_type.to_string(), block.model.to_string()))
            .unwrap_or_else(|| ("core".to_string(), "block".to_string())),
    };
    let family = block_family_for_kind(&kind).to_string();
    let block_type = supported_block_type(&kind);
    let (thumbnail, has_thumbnail, thumb_width, thumb_height) = load_thumbnail_image(&kind, &label);

    // I/O and Insert blocks are not registered effect types, so resolve icon_kind/type_label directly
    let is_io = matches!(block.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_));
    let resolved_icon_kind: String = if is_io {
        kind.clone()
    } else {
        block_type.as_ref().map(|e| e.icon_kind).unwrap_or("core").to_string()
    };
    let resolved_type_label: &str = if is_io {
        match &block.kind {
            AudioBlockKind::Input(_) => "INPUT",
            AudioBlockKind::Output(_) => "OUTPUT",
            AudioBlockKind::Insert(_) => "INSERT",
            _ => "BLOCK",
        }
    } else {
        block_type
            .as_ref()
            .map(|e| e.display_label)
            .unwrap_or("BLOCK")
    };

    let accent_color = crate::ui_state::accent_color_for_icon_kind(&resolved_icon_kind);
    ChainBlockItem {
        kind: kind.into(),
        icon_kind: resolved_icon_kind.into(),
        type_label: resolved_type_label.into(),
        label: label.into(),
        family: family.into(),
        enabled: block.enabled,
        real_index: 0,
        thumbnail,
        has_thumbnail,
        thumb_width,
        thumb_height,
        accent_color,
        icon_source: slint::Image::default(),
    }
}
fn build_input_channel_items(
    input_group: &InputGroupDraft,
    draft: &ChainDraft,
    project: &Project,
    input_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = input_group.device_id.as_ref() else {
        return Vec::new();
    };
    let Some(device) = input_devices.iter().find(|device| &device.id == device_id) else {
        return Vec::new();
    };
    let used_channels = project
        .chains
        .iter()
        .enumerate()
        .filter(|(index, chain)| {
            chain.enabled
                && draft.editing_index != Some(*index)
        })
        .flat_map(|(_, chain)| {
            chain.input_blocks().into_iter()
                .flat_map(|(_, inp)| inp.entries.iter())
                .filter(|entry| entry.device_id.0 == *device_id)
                .flat_map(|entry| entry.channels.iter().copied().collect::<Vec<_>>())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: format!("Canal {}", channel + 1).into(),
            selected: input_group.channels.contains(&channel),
            available: !used_channels.contains(&channel),
        })
        .collect()
}
fn build_output_channel_items(
    output_group: &OutputGroupDraft,
    output_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = output_group.device_id.as_ref() else {
        return Vec::new();
    };
    let Some(device) = output_devices.iter().find(|device| &device.id == device_id) else {
        return Vec::new();
    };
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: format!("Canal {}", channel + 1).into(),
            selected: output_group.channels.contains(&channel),
            available: true,
        })
        .collect()
}
fn replace_channel_options(model: &Rc<VecModel<ChannelOptionItem>>, items: Vec<ChannelOptionItem>) {
    model.set_vec(items);
}
fn build_insert_send_channel_items(
    draft: &InsertDraft,
    output_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = draft.send_device_id.as_ref() else {
        return Vec::new();
    };
    let Some(device) = output_devices.iter().find(|d| &d.id == device_id) else {
        return Vec::new();
    };
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: format!("Canal {}", channel + 1).into(),
            selected: draft.send_channels.contains(&channel),
            available: true,
        })
        .collect()
}
fn build_insert_return_channel_items(
    draft: &InsertDraft,
    input_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = draft.return_device_id.as_ref() else {
        return Vec::new();
    };
    let Some(device) = input_devices.iter().find(|d| &d.id == device_id) else {
        return Vec::new();
    };
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: format!("Canal {}", channel + 1).into(),
            selected: draft.return_channels.contains(&channel),
            available: true,
        })
        .collect()
}
fn insert_mode_to_index(mode: ChainInputMode) -> i32 {
    match mode {
        ChainInputMode::Mono => 0,
        ChainInputMode::Stereo => 1,
        ChainInputMode::DualMono => 0,
    }
}
fn insert_mode_from_index(index: i32) -> ChainInputMode {
    match index {
        1 => ChainInputMode::Stereo,
        _ => ChainInputMode::Mono,
    }
}
fn normalized_chain_description(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
fn chain_from_draft(draft: &ChainDraft, existing_chain: Option<&Chain>) -> Chain {
    if let Some(existing) = existing_chain {
        // Edit mode: only update name and instrument, preserve all blocks as-is
        Chain {
            id: existing.id.clone(),
            description: normalized_chain_description(&draft.name),
            instrument: draft.instrument.clone(),
            enabled: existing.enabled,
            blocks: existing.blocks.clone(),
        }
    } else {
        // Create mode: build initial I/O blocks from draft
        let input_entries: Vec<InputEntry> = draft
            .inputs
            .iter()
            .filter(|ig| ig.device_id.is_some() && !ig.channels.is_empty())
            .map(|ig| InputEntry {
                device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
                mode: ig.mode,
                channels: ig.channels.clone(),
            })
            .collect();
        let output_entries: Vec<OutputEntry> = draft
            .outputs
            .iter()
            .filter(|og| og.device_id.is_some() && !og.channels.is_empty())
            .map(|og| OutputEntry {
                device_id: DeviceId(og.device_id.clone().unwrap_or_default()),
                mode: og.mode,
                channels: og.channels.clone(),
            })
            .collect();

        let mut blocks = Vec::new();
        if !input_entries.is_empty() {
            blocks.push(AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".to_string(),
                    entries: input_entries,
                }),
            });
        }
        if !output_entries.is_empty() {
            blocks.push(AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".to_string(),
                    entries: output_entries,
                }),
            });
        }

        Chain {
            id: ChainId::generate(),
            description: normalized_chain_description(&draft.name),
            instrument: draft.instrument.clone(),
            enabled: false,
            blocks,
        }
    }
}
fn stop_project_runtime(project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>) {
    if let Some(mut runtime) = project_runtime.borrow_mut().take() {
        runtime.stop();
    }
}
fn sync_project_runtime(
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
fn sync_live_chain_runtime(
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
fn remove_live_chain_runtime(
    project_runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    chain_id: &ChainId,
) {
    if let Some(runtime) = project_runtime.borrow_mut().as_mut() {
        runtime.remove_chain(chain_id);
    }
}
fn assign_new_block_ids(chain: &mut Chain) {
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
const INSTRUMENT_KEYS: &[&str] = &[
    block_core::INST_ELECTRIC_GUITAR,
    block_core::INST_ACOUSTIC_GUITAR,
    block_core::INST_BASS,
    block_core::INST_VOICE,
    block_core::INST_KEYS,
    block_core::INST_DRUMS,
    block_core::INST_GENERIC,
];
fn instrument_index_to_string(index: i32) -> &'static str {
    INSTRUMENT_KEYS
        .get(index as usize)
        .copied()
        .unwrap_or(block_core::DEFAULT_INSTRUMENT)
}
fn input_mode_to_index(mode: ChainInputMode) -> i32 {
    match mode {
        ChainInputMode::Mono => 0,
        ChainInputMode::Stereo => 1,
        ChainInputMode::DualMono => 2,
    }
}
fn input_mode_from_index(index: i32) -> ChainInputMode {
    match index {
        1 => ChainInputMode::Stereo,
        2 => ChainInputMode::DualMono,
        _ => ChainInputMode::Mono,
    }
}
fn output_mode_to_index(mode: ChainOutputMode) -> i32 {
    match mode {
        ChainOutputMode::Mono => 0,
        ChainOutputMode::Stereo => 1,
    }
}
fn output_mode_from_index(index: i32) -> ChainOutputMode {
    match index {
        1 => ChainOutputMode::Stereo,
        _ => ChainOutputMode::Mono,
    }
}
fn instrument_string_to_index(instrument: &str) -> i32 {
    INSTRUMENT_KEYS
        .iter()
        .position(|&key| key == instrument)
        .map(|i| i as i32)
        .unwrap_or(0)
}
fn chain_editor_mode(draft: &ChainDraft) -> ChainEditorMode {
    if draft.editing_index.is_some() {
        ChainEditorMode::Edit
    } else {
        ChainEditorMode::Create
    }
}
fn apply_chain_editor_labels(window: &AppWindow, draft: &ChainDraft) {
    match chain_editor_mode(draft) {
        ChainEditorMode::Create => {
            window.set_chain_editor_title("Nova chain".into());
            window.set_chain_editor_save_label("Criar chain".into());
        }
        ChainEditorMode::Edit => {
            window.set_chain_editor_title("Configurar chain".into());
            window.set_chain_editor_save_label("Salvar chain".into());
        }
    }
}
fn endpoint_summary(
    device_id: Option<&str>,
    channels: &[usize],
    devices: &[AudioDeviceDescriptor],
) -> String {
    let device_name = device_id
        .and_then(|id| devices.iter().find(|device| device.id == id).map(|device| device.name.clone()))
        .or_else(|| device_id.map(|id| id.to_string()))
        .unwrap_or_else(|| "Nenhum dispositivo".to_string());
    let channels = if channels.is_empty() {
        "-".to_string()
    } else {
        channels
            .iter()
            .map(|channel| format!("{}", channel + 1))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("{device_name} · Ch {channels}")
}
/// Register all callbacks on a freshly-created `ChainEditorWindow`.
/// Called each time the chain editor is opened so the window starts with clean state.
#[allow(clippy::too_many_arguments)]
fn setup_chain_editor_callbacks(
    editor_window: &ChainEditorWindow,
    weak_window: slint::Weak<AppWindow>,
    chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    chain_input_device_options: Rc<VecModel<SharedString>>,
    chain_output_device_options: Rc<VecModel<SharedString>>,
    chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    chain_output_channels: Rc<VecModel<ChannelOptionItem>>,
    _weak_input_window: slint::Weak<ChainInputWindow>,
    _weak_output_window: slint::Weak<ChainOutputWindow>,
    io_block_insert_draft: Rc<RefCell<Option<IoBlockInsertDraft>>>,
    toast_timer: Rc<Timer>,
    auto_save: bool,
) {
    // on_update_chain_name
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        editor_window.on_update_chain_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                draft.name = value.to_string();
                window.set_chain_draft_name(value.clone());
                chain_window.set_chain_name(value);
            }
        });
    }
    // on_select_instrument
    {
        let chain_draft = chain_draft.clone();
        editor_window.on_select_instrument(move |index| {
            let instrument = instrument_index_to_string(index).to_string();
            log::debug!("[select_instrument] index={}, instrument='{}'", index, instrument);
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                draft.instrument = instrument;
                log::debug!("[select_instrument] draft updated to '{}'", draft.instrument);
            } else {
                log::warn!("[select_instrument] no draft to update!");
            }
        });
    }
    // on_edit_input
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        editor_window.on_edit_input(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let gi = group_index as usize;
            {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                draft.editing_input_index = Some(gi);
            }
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_io_groups(
                    &window,
                    &chain_window,
                    draft,
                    &fresh_input,
                    &fresh_output,
                );
                chain_window.set_input_device_options(ModelRc::from(chain_input_device_options.clone()));
                chain_window.set_input_channels(ModelRc::from(chain_input_channels.clone()));
                if let Some(input_group) = draft.inputs.get(gi) {
                    chain_window.set_input_selected_device_index(selected_device_index(
                        &fresh_input,
                        input_group.device_id.as_deref(),
                    ));
                    chain_window.set_input_mode_index(input_mode_to_index(input_group.mode));
                }
                chain_window.set_input_editor_status("".into());
                chain_window.set_show_input_editor(true);
            }
        });
    }
    // on_edit_output
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_output_channels = chain_output_channels.clone();
        editor_window.on_edit_output(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let gi = group_index as usize;
            {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                draft.editing_output_index = Some(gi);
            }
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_io_groups(
                    &window,
                    &chain_window,
                    draft,
                    &fresh_input,
                    &fresh_output,
                );
                chain_window.set_output_device_options(ModelRc::from(chain_output_device_options.clone()));
                chain_window.set_output_channels(ModelRc::from(chain_output_channels.clone()));
                if let Some(output_group) = draft.outputs.get(gi) {
                    chain_window.set_output_selected_device_index(selected_device_index(
                        &fresh_output,
                        output_group.device_id.as_deref(),
                    ));
                    chain_window.set_output_mode_index(output_mode_to_index(output_group.mode));
                }
                chain_window.set_output_editor_status("".into());
                chain_window.set_show_output_editor(true);
            }
        });
    }
    // on_add_input
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        editor_window.on_add_input(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let new_idx = {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                let idx = draft.inputs.len();
                draft.inputs.push(InputGroupDraft {
                    device_id: fresh_input.first().map(|d| d.id.clone()),
                    channels: Vec::new(),
                    mode: ChainInputMode::Mono,
                });
                draft.editing_input_index = Some(idx);
                draft.adding_new_input = true;
                if let Some(chain_window) = weak_chain_window.upgrade() {
                    apply_chain_io_groups(
                        &window,
                        &chain_window,
                        draft,
                        &fresh_input,
                        &fresh_output,
                    );
                }
                idx
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                chain_window.set_input_device_options(ModelRc::from(chain_input_device_options.clone()));
                chain_window.set_input_channels(ModelRc::from(chain_input_channels.clone()));
                if let Some(input_group) = draft.inputs.get(new_idx) {
                    chain_window.set_input_selected_device_index(selected_device_index(
                        &fresh_input,
                        input_group.device_id.as_deref(),
                    ));
                    chain_window.set_input_mode_index(input_mode_to_index(input_group.mode));
                }
                chain_window.set_input_editor_status("".into());
                chain_window.set_show_input_editor(true);
            }
        });
    }
    // on_add_output
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_output_channels = chain_output_channels.clone();
        editor_window.on_add_output(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let new_idx = {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                let idx = draft.outputs.len();
                draft.outputs.push(OutputGroupDraft {
                    device_id: fresh_output.first().map(|d| d.id.clone()),
                    channels: Vec::new(),
                    mode: ChainOutputMode::Stereo,
                });
                draft.editing_output_index = Some(idx);
                draft.adding_new_output = true;
                if let Some(chain_window) = weak_chain_window.upgrade() {
                    apply_chain_io_groups(
                        &window,
                        &chain_window,
                        draft,
                        &fresh_input,
                        &fresh_output,
                    );
                }
                idx
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                chain_window.set_output_device_options(ModelRc::from(chain_output_device_options.clone()));
                chain_window.set_output_channels(ModelRc::from(chain_output_channels.clone()));
                if let Some(output_group) = draft.outputs.get(new_idx) {
                    chain_window.set_output_selected_device_index(selected_device_index(
                        &fresh_output,
                        output_group.device_id.as_deref(),
                    ));
                    chain_window.set_output_mode_index(output_mode_to_index(output_group.mode));
                }
                chain_window.set_output_editor_status("".into());
                chain_window.set_show_output_editor(true);
            }
        });
    }
    // on_remove_input
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_remove_input(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            // Fixed block (chip In/Out): must keep at least one entry
            if draft.editing_io_block_index.is_none() && draft.inputs.len() <= 1 {
                return;
            }
            let gi = group_index as usize;
            if gi < draft.inputs.len() {
                draft.inputs.remove(gi);
                // Reset editing index if it was pointing to the removed group
                if draft.editing_input_index == Some(gi) {
                    draft.editing_input_index = None;
                } else if let Some(idx) = draft.editing_input_index {
                    if idx > gi {
                        draft.editing_input_index = Some(idx - 1);
                    }
                }
            }
            apply_chain_io_groups(
                &window,
                &chain_window,
                draft,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
        });
    }
    // on_remove_output
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_remove_output(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            // Fixed block (chip In/Out): must keep at least one entry
            if draft.editing_io_block_index.is_none() && draft.outputs.len() <= 1 {
                return;
            }
            let gi = group_index as usize;
            if gi < draft.outputs.len() {
                draft.outputs.remove(gi);
                if draft.editing_output_index == Some(gi) {
                    draft.editing_output_index = None;
                } else if let Some(idx) = draft.editing_output_index {
                    if idx > gi {
                        draft.editing_output_index = Some(idx - 1);
                    }
                }
            }
            apply_chain_io_groups(
                &window,
                &chain_window,
                draft,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
        });
    }
    // on_save_chain
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        editor_window.on_save_chain(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                chain_window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let draft = match chain_draft.borrow().clone() {
                Some(draft) => draft,
                None => {
                    chain_window.set_status_message("Nenhuma chain em edição.".into());
                    return;
                }
            };
            if draft.inputs.is_empty() {
                chain_window.set_status_message("Adicione pelo menos uma entrada.".into());
                return;
            }
            if draft.outputs.is_empty() {
                chain_window.set_status_message("Adicione pelo menos uma saída.".into());
                return;
            }
            for (i, input) in draft.inputs.iter().enumerate() {
                if input.device_id.is_none() {
                    chain_window.set_status_message(format!("Entrada {}: selecione o dispositivo.", i + 1).into());
                    return;
                }
                if input.channels.is_empty() {
                    chain_window.set_status_message(format!("Entrada {}: selecione pelo menos um canal.", i + 1).into());
                    return;
                }
            }
            for (i, output) in draft.outputs.iter().enumerate() {
                if output.device_id.is_none() {
                    chain_window.set_status_message(format!("Saída {}: selecione o dispositivo.", i + 1).into());
                    return;
                }
                if output.channels.is_empty() {
                    chain_window.set_status_message(format!("Saída {}: selecione pelo menos um canal.", i + 1).into());
                    return;
                }
            }
            let editing_index = draft.editing_index;
            log::debug!("[save_chain] editing_index={:?}, draft.instrument='{}'", editing_index, draft.instrument);
            let existing_chain =
                editing_index.and_then(|index| session.project.chains.get(index).cloned());
            let chain = chain_from_draft(&draft, existing_chain.as_ref());
            if let Err(msg) = chain.validate_channel_conflicts() {
                chain_window.set_status_message(msg.into());
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
                chain_window.set_status_message(error.to_string().into());
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
            chain_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
            let _ = chain_window.hide();
        });
    }
    // on_cancel_chain
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let toast_timer = toast_timer.clone();
        editor_window.on_cancel_chain(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            *chain_draft.borrow_mut() = None;
            chain_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
            let _ = chain_window.hide();
        });
    }
    // inline input editor: on_input_select_device
    {
        let weak_window = weak_window.clone();
        editor_window.on_input_select_device(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_chain_input_device(index);
            }
        });
    }
    // inline input editor: on_input_toggle_channel
    {
        let weak_window = weak_window.clone();
        editor_window.on_input_toggle_channel(move |index, selected| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_chain_input_channel(index, selected);
            }
        });
    }
    // inline input editor: on_input_select_mode
    {
        let chain_draft = chain_draft.clone();
        editor_window.on_input_select_mode(move |index| {
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                if let Some(gi) = draft.editing_input_index {
                    if let Some(input) = draft.inputs.get_mut(gi) {
                        input.mode = input_mode_from_index(index);
                    }
                }
            }
        });
    }
    // inline input editor: on_input_cancel
    {
        let weak_chain_window = editor_window.as_weak();
        let weak_window = weak_window.clone();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_input_cancel(move || {
            let Some(chain_window) = weak_chain_window.upgrade() else { return; };
            chain_window.set_input_editor_status("".into());
            chain_window.set_show_input_editor(false);
            if io_block_insert_draft.borrow().is_some() {
                *io_block_insert_draft.borrow_mut() = None;
                *chain_draft.borrow_mut() = None;
                return;
            }
            let mut draft_borrow = chain_draft.borrow_mut();
            if let Some(draft) = draft_borrow.as_mut() {
                if draft.adding_new_input {
                    if let Some(idx) = draft.editing_input_index {
                        if idx < draft.inputs.len() {
                            draft.inputs.remove(idx);
                        }
                    }
                    draft.adding_new_input = false;
                    draft.editing_input_index = None;
                    if let Some(window) = weak_window.upgrade() {
                        apply_chain_io_groups(&window, &chain_window, draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    }
                }
            }
        });
    }
    // inline input editor: on_input_save
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_input_save(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(chain_window) = weak_chain_window.upgrade() else { return; };
            let io_insert = io_block_insert_draft.borrow().clone();
            if let Some(io_draft) = io_insert {
                if io_draft.kind == "input" {
                    let input_group = {
                        let draft_borrow = chain_draft.borrow();
                        let Some(draft) = draft_borrow.as_ref() else {
                            *io_block_insert_draft.borrow_mut() = None;
                            chain_window.set_show_input_editor(false);
                            return;
                        };
                        let Some(ig) = draft.inputs.first().cloned() else {
                            *io_block_insert_draft.borrow_mut() = None;
                            chain_window.set_show_input_editor(false);
                            return;
                        };
                        ig
                    };
                    if input_group.device_id.is_none() || input_group.channels.is_empty() {
                        chain_window.set_input_editor_status("Selecione dispositivo e canais.".into());
                        return;
                    }
                    let chain_index = io_draft.chain_index;
                    let before_index = io_draft.before_index;
                    *io_block_insert_draft.borrow_mut() = None;
                    *chain_draft.borrow_mut() = None;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        chain_window.set_show_input_editor(false);
                        return;
                    };
                    let Some(chain) = session.project.chains.get_mut(chain_index) else {
                        chain_window.set_show_input_editor(false);
                        return;
                    };
                    let real_chain_id = chain.id.clone();
                    let input_block = AudioBlock {
                        id: BlockId::generate_for_chain(&real_chain_id),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            entries: vec![InputEntry {
                                device_id: DeviceId(input_group.device_id.clone().unwrap_or_default()),
                                mode: input_group.mode,
                                channels: input_group.channels.clone(),
                            }],
                        }),
                    };
                    let insert_pos = before_index.min(chain.blocks.len());
                    chain.blocks.insert(insert_pos, input_block);
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &real_chain_id) {
                        eprintln!("io block insert error: {error}");
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                    chain_window.set_input_editor_status("".into());
                    chain_window.set_show_input_editor(false);
                    return;
                }
            }
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                chain_window.set_show_input_editor(false);
                return;
            };
            let Some(gi) = draft.editing_input_index else {
                chain_window.set_show_input_editor(false);
                return;
            };
            let Some(input_group) = draft.inputs.get(gi) else {
                chain_window.set_show_input_editor(false);
                return;
            };
            if input_group.device_id.is_none() || input_group.channels.is_empty() {
                chain_window.set_input_editor_status("Selecione dispositivo e canais.".into());
                return;
            }
            if let Some(index) = draft.editing_index {
                let mut session_borrow = project_session.borrow_mut();
                let Some(session) = session_borrow.as_mut() else { return; };
                let Some(chain) = session.project.chains.get_mut(index) else { return; };
                let new_input_blocks: Vec<AudioBlock> = draft.inputs.iter().enumerate().map(|(i, ig)| AudioBlock {
                    id: BlockId(format!("{}:input:{}", chain.id.0, i)),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
                            mode: ig.mode,
                            channels: ig.channels.clone(),
                        }],
                    }),
                }).collect();
                let non_input_blocks: Vec<AudioBlock> = chain.blocks.iter()
                    .filter(|b| !matches!(&b.kind, AudioBlockKind::Input(_)))
                    .cloned()
                    .collect();
                let mut all_blocks = Vec::with_capacity(new_input_blocks.len() + non_input_blocks.len());
                all_blocks.extend(new_input_blocks);
                all_blocks.extend(non_input_blocks);
                chain.blocks = all_blocks;
                let chain_id = chain.id.clone();
                if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                    eprintln!("input editor save error: {error}");
                    return;
                }
                replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            }
            apply_chain_io_groups(&window, &chain_window, draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            draft.adding_new_input = false;
            chain_window.set_input_editor_status("".into());
            chain_window.set_show_input_editor(false);
        });
    }
    // inline output editor: on_output_select_device
    {
        let weak_window = weak_window.clone();
        editor_window.on_output_select_device(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_chain_output_device(index);
            }
        });
    }
    // inline output editor: on_output_toggle_channel
    {
        let weak_window = weak_window.clone();
        editor_window.on_output_toggle_channel(move |index, selected| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_chain_output_channel(index, selected);
            }
        });
    }
    // inline output editor: on_output_select_mode
    {
        let chain_draft = chain_draft.clone();
        editor_window.on_output_select_mode(move |index| {
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                if let Some(gi) = draft.editing_output_index {
                    if let Some(output) = draft.outputs.get_mut(gi) {
                        output.mode = output_mode_from_index(index);
                    }
                }
            }
        });
    }
    // inline output editor: on_output_cancel
    {
        let weak_chain_window = editor_window.as_weak();
        let weak_window = weak_window.clone();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_output_cancel(move || {
            let Some(chain_window) = weak_chain_window.upgrade() else { return; };
            chain_window.set_output_editor_status("".into());
            chain_window.set_show_output_editor(false);
            if io_block_insert_draft.borrow().is_some() {
                *io_block_insert_draft.borrow_mut() = None;
                *chain_draft.borrow_mut() = None;
                return;
            }
            let mut draft_borrow = chain_draft.borrow_mut();
            if let Some(draft) = draft_borrow.as_mut() {
                if draft.adding_new_output {
                    if let Some(idx) = draft.editing_output_index {
                        if idx < draft.outputs.len() {
                            draft.outputs.remove(idx);
                        }
                    }
                    draft.adding_new_output = false;
                    draft.editing_output_index = None;
                    if let Some(window) = weak_window.upgrade() {
                        apply_chain_io_groups(&window, &chain_window, draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    }
                }
            }
        });
    }
    // inline output editor: on_output_save
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_output_save(move || {
            let Some(window) = weak_window.upgrade() else { return; };
            let Some(chain_window) = weak_chain_window.upgrade() else { return; };
            let io_insert = io_block_insert_draft.borrow().clone();
            if let Some(io_draft) = io_insert {
                if io_draft.kind == "output" {
                    let output_group = {
                        let draft_borrow = chain_draft.borrow();
                        let Some(draft) = draft_borrow.as_ref() else {
                            *io_block_insert_draft.borrow_mut() = None;
                            chain_window.set_show_output_editor(false);
                            return;
                        };
                        let Some(og) = draft.outputs.first().cloned() else {
                            *io_block_insert_draft.borrow_mut() = None;
                            chain_window.set_show_output_editor(false);
                            return;
                        };
                        og
                    };
                    if output_group.device_id.is_none() || output_group.channels.is_empty() {
                        chain_window.set_output_editor_status("Selecione dispositivo e canais.".into());
                        return;
                    }
                    let chain_index = io_draft.chain_index;
                    let before_index = io_draft.before_index;
                    *io_block_insert_draft.borrow_mut() = None;
                    *chain_draft.borrow_mut() = None;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        chain_window.set_show_output_editor(false);
                        return;
                    };
                    let Some(chain) = session.project.chains.get_mut(chain_index) else {
                        chain_window.set_show_output_editor(false);
                        return;
                    };
                    let real_chain_id = chain.id.clone();
                    let output_block = AudioBlock {
                        id: BlockId::generate_for_chain(&real_chain_id),
                        enabled: true,
                        kind: AudioBlockKind::Output(OutputBlock {
                            model: "standard".to_string(),
                            entries: vec![OutputEntry {
                                device_id: DeviceId(output_group.device_id.clone().unwrap_or_default()),
                                mode: output_group.mode,
                                channels: output_group.channels.clone(),
                            }],
                        }),
                    };
                    let insert_pos = before_index.min(chain.blocks.len());
                    chain.blocks.insert(insert_pos, output_block);
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &real_chain_id) {
                        eprintln!("io block insert error: {error}");
                    }
                    replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                    chain_window.set_output_editor_status("".into());
                    chain_window.set_show_output_editor(false);
                    return;
                }
            }
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                chain_window.set_show_output_editor(false);
                return;
            };
            let Some(gi) = draft.editing_output_index else {
                chain_window.set_show_output_editor(false);
                return;
            };
            let Some(output_group) = draft.outputs.get(gi) else {
                chain_window.set_show_output_editor(false);
                return;
            };
            if output_group.device_id.is_none() || output_group.channels.is_empty() {
                chain_window.set_output_editor_status("Selecione dispositivo e canais.".into());
                return;
            }
            if let Some(index) = draft.editing_index {
                let mut session_borrow = project_session.borrow_mut();
                let Some(session) = session_borrow.as_mut() else { return; };
                let Some(chain) = session.project.chains.get_mut(index) else { return; };
                let new_output_blocks: Vec<AudioBlock> = draft.outputs.iter().enumerate().map(|(i, og)| AudioBlock {
                    id: BlockId(format!("{}:output:{}", chain.id.0, i)),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId(og.device_id.clone().unwrap_or_default()),
                            mode: og.mode,
                            channels: og.channels.clone(),
                        }],
                    }),
                }).collect();
                let non_output_blocks: Vec<AudioBlock> = chain.blocks.iter()
                    .filter(|b| !matches!(&b.kind, AudioBlockKind::Output(_)))
                    .cloned()
                    .collect();
                let mut all_blocks = Vec::with_capacity(non_output_blocks.len() + new_output_blocks.len());
                all_blocks.extend(non_output_blocks);
                all_blocks.extend(new_output_blocks);
                chain.blocks = all_blocks;
                let chain_id = chain.id.clone();
                if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                    eprintln!("output editor save error: {error}");
                    return;
                }
                replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            }
            apply_chain_io_groups(&window, &chain_window, draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            draft.adding_new_output = false;
            chain_window.set_output_editor_status("".into());
            chain_window.set_show_output_editor(false);
        });
    }
}
fn build_io_group_items(
    draft: &ChainDraft,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) -> (Vec<IoGroupItem>, Vec<IoGroupItem>) {
    let input_items: Vec<IoGroupItem> = draft
        .inputs
        .iter()
        .map(|input| {
            let summary = endpoint_summary(
                input.device_id.as_deref(),
                &input.channels,
                input_devices,
            );
            IoGroupItem {
                summary: summary.into(),
            }
        })
        .collect();
    let output_items: Vec<IoGroupItem> = draft
        .outputs
        .iter()
        .map(|output| {
            let summary = endpoint_summary(
                output.device_id.as_deref(),
                &output.channels,
                output_devices,
            );
            IoGroupItem {
                summary: summary.into(),
            }
        })
        .collect();
    (input_items, output_items)
}
fn apply_chain_io_groups(
    window: &AppWindow,
    chain_editor_window: &ChainEditorWindow,
    draft: &ChainDraft,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) {
    let (input_items, output_items) = build_io_group_items(draft, input_devices, output_devices);
    // Update main window summaries (first input/output for legacy compat)
    let input_summary = draft
        .inputs
        .first()
        .map(|i| endpoint_summary(i.device_id.as_deref(), &i.channels, input_devices))
        .unwrap_or_default();
    let output_summary = draft
        .outputs
        .first()
        .map(|o| endpoint_summary(o.device_id.as_deref(), &o.channels, output_devices))
        .unwrap_or_default();
    window.set_chain_input_summary(input_summary.into());
    window.set_chain_output_summary(output_summary.into());
    chain_editor_window.set_input_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
    chain_editor_window.set_output_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
}
fn apply_chain_input_window_state(
    input_window: &ChainInputWindow,
    input_group: &InputGroupDraft,
    draft: &ChainDraft,
    project: &Project,
    input_devices: &[AudioDeviceDescriptor],
    channel_model: &Rc<VecModel<ChannelOptionItem>>,
) {
    replace_channel_options(
        channel_model,
        build_input_channel_items(input_group, draft, project, input_devices),
    );
    input_window.set_selected_device_index(selected_device_index(
        input_devices,
        input_group.device_id.as_deref(),
    ));
    input_window.set_selected_input_mode_index(input_mode_to_index(input_group.mode));
    input_window.set_status_message("".into());
}
fn apply_chain_output_window_state(
    output_window: &ChainOutputWindow,
    output_group: &OutputGroupDraft,
    output_devices: &[AudioDeviceDescriptor],
    channel_model: &Rc<VecModel<ChannelOptionItem>>,
) {
    replace_channel_options(
        channel_model,
        build_output_channel_items(output_group, output_devices),
    );
    output_window.set_selected_device_index(selected_device_index(
        output_devices,
        output_group.device_id.as_deref(),
    ));
    output_window.set_selected_output_mode_index(output_mode_to_index(output_group.mode));
    output_window.set_status_message("".into());
}
fn toggle_device_row(model: &Rc<VecModel<DeviceSelectionItem>>, index: usize, selected: bool) {
    if let Some(mut row) = model.row_data(index) {
        row.selected = selected;
        model.set_row_data(index, row);
    }
}
fn update_device_sample_rate(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    index: usize,
    value: slint::SharedString,
) {
    if let Some(mut row) = model.row_data(index) {
        row.sample_rate_text = value;
        model.set_row_data(index, row);
    }
}
fn update_device_buffer_size(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    index: usize,
    value: slint::SharedString,
) {
    if let Some(mut row) = model.row_data(index) {
        row.buffer_size_text = value;
        model.set_row_data(index, row);
    }
}
fn update_device_bit_depth(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    index: usize,
    value: slint::SharedString,
) {
    if let Some(mut row) = model.row_data(index) {
        row.bit_depth_text = value;
        model.set_row_data(index, row);
    }
}
fn selected_device_settings(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    device_kind: &str,
) -> Result<Vec<GuiAudioDeviceSettings>> {
    (0..model.row_count())
        .filter_map(|index| model.row_data(index))
        .filter(|row| row.selected)
        .map(|row| {
            Ok(GuiAudioDeviceSettings {
                device_id: row.device_id.to_string(),
                name: row.name.to_string(),
                sample_rate: parse_positive_u32(
                    row.sample_rate_text.as_str(),
                    &format!("{}_sample_rate '{}'", device_kind, row.name),
                )?,
                buffer_size_frames: parse_positive_u32(
                    row.buffer_size_text.as_str(),
                    &format!("{}_buffer_size_frames '{}'", device_kind, row.name),
                )?,
                bit_depth: parse_positive_u32(
                    row.bit_depth_text.as_str(),
                    &format!("{}_bit_depth '{}'", device_kind, row.name),
                )?,
            })
        })
        .collect()
}
fn default_device_settings(device_id: String, name: String) -> GuiAudioDeviceSettings {
    GuiAudioDeviceSettings {
        device_id,
        name,
        sample_rate: DEFAULT_SAMPLE_RATE,
        buffer_size_frames: DEFAULT_BUFFER_SIZE_FRAMES,
        bit_depth: DEFAULT_BIT_DEPTH,
    }
}
fn normalize_device_settings(mut settings: GuiAudioDeviceSettings) -> GuiAudioDeviceSettings {
    if !SUPPORTED_SAMPLE_RATES.contains(&settings.sample_rate) {
        settings.sample_rate = DEFAULT_SAMPLE_RATE;
    }
    if !SUPPORTED_BUFFER_SIZES.contains(&settings.buffer_size_frames) {
        settings.buffer_size_frames = DEFAULT_BUFFER_SIZE_FRAMES;
    }
    if !SUPPORTED_BIT_DEPTHS.contains(&settings.bit_depth) {
        settings.bit_depth = DEFAULT_BIT_DEPTH;
    }
    settings
}
fn mark_unselected_devices(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    selected_devices: &[GuiAudioDeviceSettings],
) {
    for index in 0..model.row_count() {
        let Some(mut row) = model.row_data(index) else {
            continue;
        };
        row.selected = selected_devices
            .iter()
            .any(|saved| saved.device_id == row.device_id.as_str());
        model.set_row_data(index, row);
    }
}
fn parse_positive_u32(value: &str, field: &str) -> Result<u32> {
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| anyhow!("'{}' inválido: '{}'", field, value))
}
#[cfg(test)]
mod tests {
    use super::{
        block_editor_data, block_parameter_items_for_editor, numeric_widget_kind,
        open_cli_project, parse_cli_args_from, quantize_numeric_value, SELECT_SELECTED_BLOCK_ID,
    };
    use domain::ids::BlockId;
    use domain::value_objects::ParameterValue;
    use project::catalog::supported_block_models;
    use project::block::{
        schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, SelectBlock,
    };
    use project::param::ParameterSet;
    use slint::Model;
    #[test]
    fn quantize_numeric_value_respects_float_step_and_bounds() {
        assert_eq!(quantize_numeric_value(19.64, 0.0, 100.0, 0.5, false), 19.5);
        assert_eq!(quantize_numeric_value(101.0, 0.0, 100.0, 0.5, false), 100.0);
        assert_eq!(quantize_numeric_value(-1.0, 0.0, 100.0, 0.5, false), 0.0);
    }
    #[test]
    fn quantize_numeric_value_respects_integer_step() {
        assert_eq!(
            quantize_numeric_value(243.0, 64.0, 1024.0, 64.0, true),
            256.0
        );
        assert_eq!(
            quantize_numeric_value(96.0, 64.0, 1024.0, 64.0, true),
            128.0
        );
    }
    #[test]
    fn numeric_widget_kind_prefers_stepper_for_sparse_ranges() {
        assert_eq!(numeric_widget_kind(50.0, 70.0, 10.0, false), "stepper");
        assert_eq!(numeric_widget_kind(10.0, 100.0, 10.0, false), "stepper");
    }
    #[test]
    fn numeric_widget_kind_uses_slider_for_dense_ranges() {
        assert_eq!(numeric_widget_kind(0.0, 5.0, 0.01, false), "slider");
        assert_eq!(numeric_widget_kind(1.0, 10.0, 0.1, false), "slider");
    }
    #[test]
    fn numeric_widget_kind_prefers_slider_for_large_ranges() {
        assert_eq!(numeric_widget_kind(0.0, 100.0, 0.5, false), "slider");
        assert_eq!(numeric_widget_kind(20.0, 20000.0, 1.0, false), "slider");
    }
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

    use super::format_channel_list;

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

    use super::unit_label;
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

    // --- db_to_linear / linear_to_db ---

    use super::{db_to_linear, linear_to_db};

    #[test]
    fn db_to_linear_zero_db_is_unity() {
        let result = db_to_linear(0.0);
        assert!((result - 1.0).abs() < 1e-6);
    }

    #[test]
    fn db_to_linear_minus_20_is_point_one() {
        let result = db_to_linear(-20.0);
        assert!((result - 0.1).abs() < 1e-6);
    }

    #[test]
    fn db_to_linear_plus_20_is_ten() {
        let result = db_to_linear(20.0);
        assert!((result - 10.0).abs() < 1e-4);
    }

    #[test]
    fn linear_to_db_roundtrip() {
        for db in [-12.0_f32, -6.0, 0.0, 3.0, 6.0, 12.0] {
            let lin = db_to_linear(db);
            let back = linear_to_db(lin);
            assert!((back - db).abs() < 1e-4, "roundtrip failed for {} dB: got {}", db, back);
        }
    }

    // --- freq_to_x / gain_to_y ---

    use super::{freq_to_x, gain_to_y};

    #[test]
    fn freq_to_x_min_freq_returns_zero() {
        let x = freq_to_x(20.0);
        assert_eq!(x, 0.0);
    }

    #[test]
    fn freq_to_x_max_freq_returns_svg_width() {
        let x = freq_to_x(20_000.0);
        assert_eq!(x, 1000.0);
    }

    #[test]
    fn gain_to_y_zero_db_returns_mid_height() {
        let y = gain_to_y(0.0);
        assert_eq!(y, 100.0); // EQ_SVG_H / 2
    }

    #[test]
    fn gain_to_y_max_gain_returns_zero() {
        let y = gain_to_y(24.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn gain_to_y_min_gain_returns_svg_height() {
        let y = gain_to_y(-24.0);
        assert_eq!(y, 200.0);
    }

    // --- biquad_kind_for_group ---

    use super::biquad_kind_for_group;

    #[test]
    fn biquad_kind_low_group_returns_low_shelf() {
        assert!(matches!(biquad_kind_for_group("Low Band"), block_core::BiquadKind::LowShelf));
        assert!(matches!(biquad_kind_for_group("low"), block_core::BiquadKind::LowShelf));
    }

    #[test]
    fn biquad_kind_high_group_returns_high_shelf() {
        assert!(matches!(biquad_kind_for_group("High Band"), block_core::BiquadKind::HighShelf));
        assert!(matches!(biquad_kind_for_group("HIGH"), block_core::BiquadKind::HighShelf));
    }

    #[test]
    fn biquad_kind_mid_group_returns_peak() {
        assert!(matches!(biquad_kind_for_group("Mid"), block_core::BiquadKind::Peak));
        assert!(matches!(biquad_kind_for_group(""), block_core::BiquadKind::Peak));
    }

    // --- insert_mode_to_index / insert_mode_from_index ---

    use super::{insert_mode_to_index, insert_mode_from_index};

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

    use super::normalized_chain_description;

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

    use super::preset_id_from_path;

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
    fn selected_device_index_returns_negative_when_not_found() {
        let devices = vec![
            AudioDeviceDescriptor { id: "dev_a".into(), name: "A".into(), channels: 2 },
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

    use super::real_block_index_to_ui;

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

    // --- quantize_numeric_value edge cases ---

    #[test]
    fn quantize_numeric_value_zero_step_only_clamps() {
        assert_eq!(quantize_numeric_value(50.0, 0.0, 100.0, 0.0, false), 50.0);
        assert_eq!(quantize_numeric_value(150.0, 0.0, 100.0, 0.0, false), 100.0);
    }

    #[test]
    fn quantize_numeric_value_exact_boundary_stays() {
        assert_eq!(quantize_numeric_value(0.0, 0.0, 100.0, 10.0, false), 0.0);
        assert_eq!(quantize_numeric_value(100.0, 0.0, 100.0, 10.0, false), 100.0);
    }

    #[test]
    fn quantize_numeric_value_integer_flag_rounds() {
        assert_eq!(quantize_numeric_value(3.7, 0.0, 10.0, 0.0, true), 4.0);
        assert_eq!(quantize_numeric_value(3.2, 0.0, 10.0, 0.0, true), 3.0);
    }

    // --- numeric_widget_kind edge cases ---

    #[test]
    fn numeric_widget_kind_step_zero_returns_slider() {
        assert_eq!(numeric_widget_kind(0.0, 100.0, 0.0, false), "slider");
    }

    #[test]
    fn numeric_widget_kind_boundary_24_steps_is_stepper() {
        // exactly 24 steps: (24.0 - 0.0) / 1.0 = 24
        assert_eq!(numeric_widget_kind(0.0, 24.0, 1.0, false), "stepper");
    }

    #[test]
    fn numeric_widget_kind_25_steps_is_slider() {
        assert_eq!(numeric_widget_kind(0.0, 25.0, 1.0, false), "slider");
    }

    #[test]
    fn numeric_widget_kind_equal_min_max_returns_slider() {
        assert_eq!(numeric_widget_kind(5.0, 5.0, 1.0, false), "slider");
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

    use super::chain_endpoint_label;

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

    use super::sync_recent_projects;
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

    use super::mark_recent_project_invalid;

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

    use super::chain_inputs_tooltip;

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

    use super::chain_outputs_tooltip;

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

    // --- eq_frequencies ---

    use super::eq_frequencies;

    #[test]
    fn eq_frequencies_returns_200_points() {
        let freqs = eq_frequencies();
        assert_eq!(freqs.len(), 200);
    }

    #[test]
    fn eq_frequencies_starts_at_20_hz_ends_at_20k_hz() {
        let freqs = eq_frequencies();
        assert!((freqs[0] - 20.0).abs() < 0.1);
        assert!((freqs[199] - 20_000.0).abs() < 1.0);
    }

    #[test]
    fn eq_frequencies_monotonically_increasing() {
        let freqs = eq_frequencies();
        for i in 1..freqs.len() {
            assert!(freqs[i] > freqs[i - 1], "freq[{}]={} must be > freq[{}]={}", i, freqs[i], i - 1, freqs[i - 1]);
        }
    }

    // --- db_vec_to_svg_path ---

    use super::db_vec_to_svg_path;

    #[test]
    fn db_vec_to_svg_path_starts_with_move_command() {
        let dbs = vec![0.0; 200];
        let path = db_vec_to_svg_path(&dbs);
        assert!(path.starts_with("M "), "SVG path should start with M: {}", &path[..20]);
    }

    #[test]
    fn db_vec_to_svg_path_contains_line_commands() {
        let dbs = vec![0.0; 200];
        let path = db_vec_to_svg_path(&dbs);
        assert!(path.contains(" L "), "SVG path should contain L commands");
    }

    #[test]
    fn db_vec_to_svg_path_empty_dbs_returns_empty() {
        let path = db_vec_to_svg_path(&[]);
        assert!(path.is_empty());
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
}