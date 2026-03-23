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
    load_chain_preset_file, save_chain_preset_file, serialize_audio_blocks, ChainBlocksPreset,
    YamlProjectRepository,
};
use project::block::{
    build_audio_block_kind, schema_for_block_model, AudioBlock, AudioBlockKind,
};
use project::catalog::{supported_block_models, supported_block_type, supported_block_types};
use project::device::DeviceSettings;
use project::param::{ParameterDomain, ParameterSet, ParameterUnit};
use project::project::Project;
use project::chain::{Chain, ChainInputMode, ChainOutputMixdown};
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
const SUPPORTED_SAMPLE_RATES: &[u32] = &[44_100, 48_000, 88_200, 96_000];
const SUPPORTED_BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];
fn log_gui_message(context: &str, message: &str) {
    log::info!("[adapter-gui] {context}: {message}");
}
fn log_gui_error(context: &str, error: impl Display) {
    log::error!("[adapter-gui] {context}: {error}");
}
fn use_inline_block_editor(window: &AppWindow) -> bool {
    window.get_touch_optimized()
        && window
            .get_interaction_mode_label()
            .to_string()
            .to_lowercase()
            .contains("touch")
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
    input_chain_devices: Rc<Vec<AudioDeviceDescriptor>>,
    output_chain_devices: Rc<Vec<AudioDeviceDescriptor>>,
    context: &'static str,
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
        if let Err(error) = persist_block_editor_draft(
            &window,
            &draft,
            &block_parameter_items,
            &project_session,
            &project_chains,
            &project_runtime,
            &saved_project_snapshot,
            &project_dirty,
            &input_chain_devices,
            &output_chain_devices,
            false,
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
    input_chain_devices: Rc<Vec<AudioDeviceDescriptor>>,
    output_chain_devices: Rc<Vec<AudioDeviceDescriptor>>,
    context: &'static str,
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
        if let Err(error) = persist_block_editor_draft(
            &main_window,
            &draft,
            &block_parameter_items,
            &project_session,
            &project_chains,
            &project_runtime,
            &saved_project_snapshot,
            &project_dirty,
            &input_chain_devices,
            &output_chain_devices,
            false,
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
struct ChainDraft {
    editing_index: Option<usize>,
    name: String,
    instrument: String,
    input_device_id: Option<String>,
    output_device_id: Option<String>,
    input_channels: Vec<usize>,
    output_channels: Vec<usize>,
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
struct ProjectYaml {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    device_settings: Vec<ProjectDeviceSettingsYaml>,
    chains: Vec<ProjectChainYaml>,
}
#[derive(Debug, Serialize)]
struct ProjectDeviceSettingsYaml {
    device_id: String,
    sample_rate: u32,
    buffer_size_frames: u32,
}
#[derive(Debug, Serialize)]
struct ProjectChainYaml {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    instrument: String,
    input_device_id: String,
    input_channels: Vec<usize>,
    output_device_id: String,
    output_channels: Vec<usize>,
    blocks: Vec<serde_yaml::Value>,
    output_mixdown: ChainOutputMixdown,
    input_mode: ChainInputMode,
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
pub fn run_desktop_app(
    runtime_mode: AppRuntimeMode,
    interaction_mode: InteractionMode,
) -> Result<()> {
    log::info!("starting desktop app: runtime_mode={:?}, interaction_mode={:?}", runtime_mode, interaction_mode);
    let context = UiRuntimeContext::new(runtime_mode, interaction_mode);
    let settings = FilesystemStorage::load_gui_audio_settings()?.unwrap_or_default();
    let needs_audio_settings =
        context.capabilities.can_select_audio_device && !settings.is_complete();
    let project_paths = resolve_project_paths();
    let app_config = Rc::new(RefCell::new(load_and_sync_app_config()?));
    let project_session = Rc::new(RefCell::new(None::<ProjectSession>));
    let chain_draft = Rc::new(RefCell::new(None::<ChainDraft>));
    let selected_block = Rc::new(RefCell::new(None::<SelectedBlock>));
    let block_editor_draft = Rc::new(RefCell::new(None::<BlockEditorDraft>));
    let project_runtime = Rc::new(RefCell::new(None::<ProjectRuntimeController>));
    let saved_project_snapshot = Rc::new(RefCell::new(None::<String>));
    let project_dirty = Rc::new(RefCell::new(false));
    let open_block_windows: Rc<RefCell<Vec<BlockWindow>>> = Rc::new(RefCell::new(Vec::new()));
    let audio_settings_mode = Rc::new(RefCell::new(AudioSettingsMode::Gui));
    let input_chain_devices = Rc::new(list_input_device_descriptors()?);
    let output_chain_devices = Rc::new(list_output_device_descriptors()?);
    let window = AppWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let project_settings_window =
        ProjectSettingsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let chain_editor_window =
        ChainEditorWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let chain_input_window =
        ChainInputWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let chain_output_window =
        ChainOutputWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let block_editor_window =
        BlockEditorWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    window.set_show_project_launcher(true);
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
    window.set_show_audio_settings(needs_audio_settings);
    window.set_wizard_step(if settings.is_complete() { 1 } else { 0 });
    window.set_status_message("".into());
    let input_devices = Rc::new(VecModel::from(
        list_input_device_descriptors()?
            .into_iter()
            .map(|device| {
                let device_id = device.id;
                let name = device.name;
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
                }
            })
            .collect::<Vec<_>>(),
    ));
    mark_unselected_devices(&input_devices, &settings.input_devices);
    let output_devices = Rc::new(VecModel::from(
        list_output_device_descriptors()?
            .into_iter()
            .map(|device| {
                let device_id = device.id;
                let name = device.name;
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
                }
            })
            .collect::<Vec<_>>(),
    ));
    mark_unselected_devices(&output_devices, &settings.output_devices);
    let project_devices = Rc::new(VecModel::from(build_project_device_rows(
        input_chain_devices.as_ref(),
        output_chain_devices.as_ref(),
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
    let chain_input_device_options = Rc::new(VecModel::from(
        input_chain_devices
            .iter()
            .map(|device| SharedString::from(device.name.clone()))
            .collect::<Vec<_>>(),
    ));
    let chain_output_device_options = Rc::new(VecModel::from(
        output_chain_devices
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
    chain_editor_window.set_status_message("".into());
    project_settings_window.set_project_name_draft("".into());
    chain_editor_window.set_chain_name("".into());
    let block_type_options = Rc::new(VecModel::from(block_type_picker_items(block_core::INST_GENERIC)));
    let block_model_options = Rc::new(VecModel::from(Vec::<BlockModelPickerItem>::new()));
    let block_model_option_labels = Rc::new(VecModel::from(Vec::<SharedString>::new()));
    let block_parameter_items = Rc::new(VecModel::from(Vec::<BlockParameterItem>::new()));
    let block_editor_persist_timer = Rc::new(Timer::default());
    let toast_timer = Rc::new(Timer::default());
    window.set_toast_message("".into());
    window.set_toast_level("info".into());
    window.set_block_type_options(ModelRc::from(block_type_options.clone()));
    window.set_block_model_options(ModelRc::from(block_model_options.clone()));
    window.set_block_model_option_labels(ModelRc::from(block_model_option_labels.clone()));
    window.set_block_parameter_items(ModelRc::from(block_parameter_items.clone()));
    block_editor_window.set_block_type_options(ModelRc::from(block_type_options.clone()));
    block_editor_window.set_block_model_options(ModelRc::from(block_model_options.clone()));
    block_editor_window
        .set_block_model_option_labels(ModelRc::from(block_model_option_labels.clone()));
    block_editor_window.set_block_parameter_items(ModelRc::from(block_parameter_items.clone()));
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
    project_settings_window.set_project_devices(ModelRc::from(project_devices.clone()));
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
            match *audio_settings_mode.borrow() {
                AudioSettingsMode::Gui => {
                    match FilesystemStorage::save_gui_audio_settings(&settings) {
                        Ok(()) => {
                            clear_status(&window, &toast_timer);
                            window.set_show_audio_settings(false);
                        }
                        Err(error) => set_status_error(&window, &toast_timer, &error.to_string()),
                    }
                }
                AudioSettingsMode::Project => {
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                        return;
                    };
                    session.project.device_settings = settings
                        .input_devices
                        .into_iter()
                        .chain(settings.output_devices)
                        .map(|device| DeviceSettings {
                            device_id: DeviceId(device.device_id),
                            sample_rate: device.sample_rate,
                            buffer_size_frames: device.buffer_size_frames,
                        })
                        .collect();
                    if let Err(error) = sync_project_runtime(&project_runtime, session) {
                        set_status_error(&window, &toast_timer, &error.to_string());
                        return;
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &input_chain_devices,
                        &output_chain_devices,
                    );
                    window.set_project_title(
                        project_title_for_path(session.project_path.as_ref(), &session.project)
                            .into(),
                    );
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
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
                        })
                        .collect();
                    if let Err(error) = sync_project_runtime(&project_runtime, session) {
                        settings_window.set_status_message(error.to_string().into());
                        return;
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &input_chain_devices,
                        &output_chain_devices,
                    );
                    window.set_project_title(
                        project_title_for_path(session.project_path.as_ref(), &session.project)
                            .into(),
                    );
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
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
                        &input_chain_devices,
                        &output_chain_devices,
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
                    set_status_error(&window, &toast_timer, &error.to_string());
                }
            }
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
        window.on_create_project_file(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            stop_project_runtime(&project_runtime);
            let session = create_new_project_session(&project_paths.default_config_path);
            replace_project_chains(
                &project_chains,
                &session.project,
                &input_chain_devices,
                &output_chain_devices,
            );
            *project_session.borrow_mut() = Some(session);
            *saved_project_snapshot.borrow_mut() = None;
            clear_status(&window, &toast_timer);
            set_project_dirty(&window, &project_dirty, true);
            window.set_project_title("Novo Projeto".into());
            window.set_project_name_draft("".into());
            window.set_project_path_label("Projeto em memória".into());
            window.set_show_project_launcher(false);
            window.set_show_project_chains(true);
            window.set_show_chain_editor(false);
            window.set_show_project_settings(false);
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
                        &input_chain_devices,
                        &output_chain_devices,
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
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
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
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            project_devices.set_vec(build_project_device_rows(
                input_chain_devices.as_ref(),
                output_chain_devices.as_ref(),
                &session.project.device_settings,
            ));
            *audio_settings_mode.borrow_mut() = AudioSettingsMode::Project;
            window.set_project_name_draft(session.project.name.clone().unwrap_or_default().into());
            settings_window
                .set_project_name_draft(session.project.name.clone().unwrap_or_default().into());
            settings_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_project_settings(true);
            let _ = settings_window.show();
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
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
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
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
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
                            &input_chain_devices,
                            &output_chain_devices,
                        );
                        sync_project_dirty(
                            &window,
                            session,
                            &saved_project_snapshot,
                            &project_dirty,
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
        let chain_editor_window = chain_editor_window.as_weak();
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
            if let Some(editor_window) = chain_editor_window.upgrade() {
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
                &input_chain_devices,
                &output_chain_devices,
            );
            clear_status(&window, &toast_timer);
            set_project_dirty(&window, &project_dirty, false);
            window.set_project_title("Projeto".into());
            window.set_project_name_draft("".into());
            window.set_project_path_label("".into());
            window.set_show_project_settings(false);
            window.set_show_chain_editor(false);
            window.set_show_project_chains(false);
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
        let chain_editor_window = chain_editor_window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_add_chain(move || {
            log::info!("on_add_chain triggered");
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(editor_window) = chain_editor_window.upgrade() else {
                return;
            };
            let borrow = project_session.borrow();
            let Some(session) = borrow.as_ref() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let draft = create_chain_draft(
                &session.project,
                &input_chain_devices,
                &output_chain_devices,
            );
            *chain_draft.borrow_mut() = Some(draft.clone());
            apply_chain_editor_labels(&window, &draft);
            apply_chain_endpoint_summaries(
                &window,
                &editor_window,
                &draft,
                &input_chain_devices,
                &output_chain_devices,
            );
            replace_channel_options(
                &chain_input_channels,
                build_input_channel_items(&draft, &session.project, &input_chain_devices),
            );
            replace_channel_options(
                &chain_output_channels,
                build_output_channel_items(&draft, &output_chain_devices),
            );
            window.set_chain_draft_name(draft.name.clone().into());
            editor_window.set_chain_name(draft.name.clone().into());
            editor_window.set_editor_title(window.get_chain_editor_title());
            editor_window.set_editor_save_label(window.get_chain_editor_save_label());
            editor_window.set_is_create_mode(true);
            editor_window.set_selected_instrument_index(instrument_string_to_index(&draft.instrument));
            window.set_selected_chain_input_device_index(selected_device_index(
                &input_chain_devices,
                draft.input_device_id.as_deref(),
            ));
            window.set_selected_chain_output_device_index(selected_device_index(
                &output_chain_devices,
                draft.output_device_id.as_deref(),
            ));
            editor_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(true);
            let _ = editor_window.show();
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
        let chain_editor_window = chain_editor_window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_configure_chain(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(editor_window) = chain_editor_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                set_status_error(&window, &toast_timer, "Chain inválida.");
                return;
            };
            let draft = chain_draft_from_chain(index as usize, chain);
            replace_channel_options(
                &chain_input_channels,
                build_input_channel_items(&draft, &session.project, &input_chain_devices),
            );
            replace_channel_options(
                &chain_output_channels,
                build_output_channel_items(&draft, &output_chain_devices),
            );
            window.set_chain_draft_name(draft.name.clone().into());
            editor_window.set_chain_name(draft.name.clone().into());
            window.set_selected_chain_input_device_index(selected_device_index(
                &input_chain_devices,
                draft.input_device_id.as_deref(),
            ));
            window.set_selected_chain_output_device_index(selected_device_index(
                &output_chain_devices,
                draft.output_device_id.as_deref(),
            ));
            *chain_draft.borrow_mut() = Some(draft);
            if let Some(draft) = chain_draft.borrow().as_ref() {
                apply_chain_editor_labels(&window, draft);
                apply_chain_endpoint_summaries(
                    &window,
                    &editor_window,
                    draft,
                    &input_chain_devices,
                    &output_chain_devices,
                );
                editor_window.set_editor_title(window.get_chain_editor_title());
                editor_window.set_editor_save_label(window.get_chain_editor_save_label());
                editor_window.set_is_create_mode(false);
                editor_window.set_selected_instrument_index(instrument_string_to_index(&draft.instrument));
            }
            editor_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(true);
            let _ = editor_window.show();
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
        let weak_window = window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        chain_editor_window.on_update_chain_name(move |value| {
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
    {
        let chain_draft = chain_draft.clone();
        chain_editor_window.on_select_instrument(move |index| {
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
    {
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_input_channels = chain_input_channels.clone();
        window.on_select_chain_input_device(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let Some(device) = input_chain_devices.get(index as usize) else {
                return;
            };
            draft.input_device_id = Some(device.id.clone());
            draft.input_channels.clear();
            if let Some(session) = project_session.borrow().as_ref() {
                replace_channel_options(
                    &chain_input_channels,
                    build_input_channel_items(draft, &session.project, &input_chain_devices),
                );
                if let Some(chain_window) = weak_chain_window.upgrade() {
                    apply_chain_endpoint_summaries(
                        &window,
                        &chain_window,
                        draft,
                        &input_chain_devices,
                        &output_chain_devices,
                    );
                }
            }
            let selected_index = selected_device_index(&input_chain_devices, draft.input_device_id.as_deref());
            window.set_selected_chain_input_device_index(selected_index);
            if let Some(input_window) = weak_input_window.upgrade() {
                input_window.set_selected_device_index(selected_index);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_output_channels = chain_output_channels.clone();
        window.on_select_chain_output_device(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let Some(device) = output_chain_devices.get(index as usize) else {
                return;
            };
            draft.output_device_id = Some(device.id.clone());
            draft.output_channels.clear();
            if project_session.borrow().as_ref().is_some() {
                replace_channel_options(
                    &chain_output_channels,
                    build_output_channel_items(draft, &output_chain_devices),
                );
                if let Some(chain_window) = weak_chain_window.upgrade() {
                    apply_chain_endpoint_summaries(
                        &window,
                        &chain_window,
                        draft,
                        &input_chain_devices,
                        &output_chain_devices,
                    );
                }
            }
            let selected_index = selected_device_index(&output_chain_devices, draft.output_device_id.as_deref());
            window.set_selected_chain_output_device_index(selected_index);
            if let Some(output_window) = weak_output_window.upgrade() {
                output_window.set_selected_device_index(selected_index);
            }
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_input_channels = chain_input_channels.clone();
        let weak_window = window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
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
            if selected {
                if !draft.input_channels.contains(&channel) {
                    draft.input_channels.push(channel);
                    draft.input_channels.sort_unstable();
                }
            } else {
                draft.input_channels.retain(|current| *current != channel);
            }
            if let Some(session) = project_session.borrow().as_ref() {
                replace_channel_options(
                    &chain_input_channels,
                    build_input_channel_items(draft, &session.project, &input_chain_devices),
                );
                if let Some(chain_window) = weak_chain_window.upgrade() {
                    apply_chain_endpoint_summaries(
                        &window,
                        &chain_window,
                        draft,
                        &input_chain_devices,
                        &output_chain_devices,
                    );
                }
            }
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_output_channels = chain_output_channels.clone();
        let weak_window = window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        window.on_toggle_chain_output_channel(move |index, selected| {
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let channel = index as usize;
            if selected {
                if !draft.output_channels.contains(&channel) {
                    draft.output_channels.push(channel);
                    draft.output_channels.sort_unstable();
                }
            } else {
                draft.output_channels.retain(|current| *current != channel);
            }
            if project_session.borrow().as_ref().is_some() {
                replace_channel_options(
                    &chain_output_channels,
                    build_output_channel_items(draft, &output_chain_devices),
                );
                if let (Some(window), Some(chain_window)) = (weak_window.upgrade(), weak_chain_window.upgrade()) {
                    apply_chain_endpoint_summaries(
                        &window,
                        &chain_window,
                        draft,
                        &input_chain_devices,
                        &output_chain_devices,
                    );
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_input_channels = chain_input_channels.clone();
        window.on_configure_chain_input(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(input_window) = weak_input_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                return;
            };
            let draft = chain_draft_from_chain(index as usize, chain);
            *chain_draft.borrow_mut() = Some(draft.clone());
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_endpoint_summaries(
                    &window,
                    &chain_window,
                    &draft,
                    &input_chain_devices,
                    &output_chain_devices,
                );
            }
            apply_chain_input_window_state(
                &input_window,
                &draft,
                &session.project,
                &input_chain_devices,
                &chain_input_channels,
            );
            let _ = input_window.show();
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_output_channels = chain_output_channels.clone();
        window.on_configure_chain_output(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(output_window) = weak_output_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                return;
            };
            let draft = chain_draft_from_chain(index as usize, chain);
            *chain_draft.borrow_mut() = Some(draft.clone());
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_endpoint_summaries(
                    &window,
                    &chain_window,
                    &draft,
                    &input_chain_devices,
                    &output_chain_devices,
                );
            }
            apply_chain_output_window_state(
                &output_window,
                &draft,
                &output_chain_devices,
                &chain_output_channels,
            );
            let _ = output_window.show();
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_input_channels = chain_input_channels.clone();
        chain_editor_window.on_open_input(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(input_window) = weak_input_window.upgrade() else {
                return;
            };
            let Some(draft) = chain_draft.borrow().clone() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_endpoint_summaries(
                    &window,
                    &chain_window,
                    &draft,
                    &input_chain_devices,
                    &output_chain_devices,
                );
            }
            apply_chain_input_window_state(
                &input_window,
                &draft,
                &session.project,
                &input_chain_devices,
                &chain_input_channels,
            );
            let _ = input_window.show();
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_output_channels = chain_output_channels.clone();
        chain_editor_window.on_open_output(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(output_window) = weak_output_window.upgrade() else {
                return;
            };
            let Some(draft) = chain_draft.borrow().clone() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(_session) = session_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_endpoint_summaries(
                    &window,
                    &chain_window,
                    &draft,
                    &input_chain_devices,
                    &output_chain_devices,
                );
            }
            apply_chain_output_window_state(
                &output_window,
                &draft,
                &output_chain_devices,
                &chain_output_channels,
            );
            let _ = output_window.show();
        });
    }
    {
        let weak_main_window = window.as_weak();
        let selected_block = selected_block.clone();
        let block_editor_draft = block_editor_draft.clone();
        let block_model_options = block_model_options.clone();
        let block_model_option_labels = block_model_option_labels.clone();
        let block_parameter_items = block_parameter_items.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let open_block_windows = open_block_windows.clone();
        let toast_timer = toast_timer.clone();
        window.on_select_chain_block(move |chain_index, block_index| {
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
            let Some(block) = chain.blocks.get(block_index as usize) else {
                set_status_error(&window, &toast_timer, "Block inválido.");
                return;
            };
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
                log::debug!("[select_chain_block]   model='{}'", item.model_id);
            }
            block_model_option_labels.set_vec(block_model_picker_labels(&items));
            block_model_options.set_vec(items);
            block_parameter_items.set_vec(block_parameter_items_for_editor(&editor_data));
            set_selected_block(&window, selected_block.borrow().as_ref());
            let drawer_state =
                block_drawer_state(Some(block_index as usize), &effect_type, Some(&model_id));
            window.set_block_drawer_title(drawer_state.title.into());
            window.set_block_drawer_confirm_label(drawer_state.confirm_label.into());
            window.set_block_drawer_edit_mode(true);
            window.set_block_drawer_selected_type_index(block_type_index(&effect_type, &instrument));
            window
                .set_block_drawer_selected_model_index(block_model_index(&effect_type, &model_id, &instrument));
            window.set_block_drawer_enabled(enabled);
            window.set_block_drawer_status_message("".into());
            window.set_show_block_type_picker(false);
            drop(session_borrow);
            if use_inline_block_editor(&window) {
                window.set_show_block_drawer(true);
            } else {
                window.set_show_block_drawer(false);
                let ci = chain_index as usize;
                let bi = block_index as usize;
                // Check if a window for this block already exists
                let existing = open_block_windows.borrow().iter()
                    .find(|bw| bw.chain_index == ci && bw.block_index == bi)
                    .map(|bw| bw.window.as_weak());
                if let Some(weak_win) = existing {
                    if let Some(win) = weak_win.upgrade() {
                        // Just show — window already has its own independent state
                        let _ = win.show();
                        return;
                    }
                }
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
                // on_choose_block_model
                {
                    let win_draft = win_draft.clone();
                    let win_param_items = win_param_items.clone();
                    let win_knob_overlays = win_knob_overlays.clone();
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
                        let Some(_win) = weak_win.upgrade() else { return; };
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
                        drop(draft_borrow);
                        if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                            schedule_block_editor_persist_for_block_win(
                                &win_timer, weak_win.clone(), weak_main.clone(),
                                win_draft.clone(), win_param_items.clone(),
                                project_session.clone(), project_chains.clone(), project_runtime.clone(),
                                saved_project_snapshot.clone(), project_dirty.clone(),
                                input_chain_devices.clone(), output_chain_devices.clone(),
                                "block-window.choose-model",
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
                        replace_project_chains(&project_chains, &session.project, &input_chain_devices, &output_chain_devices);
                        sync_project_dirty(&main, session, &saved_project_snapshot, &project_dirty);
                        drop(session_borrow);
                        win.set_block_drawer_enabled(new_enabled);
                    });
                }
                // on_update_block_parameter_number
                {
                    let win_draft = win_draft.clone();
                    let win_param_items = win_param_items.clone();
                    let win_knob_overlays = win_knob_overlays.clone();
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
                        let Some(_win) = weak_win.upgrade() else { return; };
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
                        if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                            schedule_block_editor_persist_for_block_win(
                                &win_timer, weak_win.clone(), weak_main.clone(),
                                win_draft.clone(), win_param_items.clone(),
                                project_session.clone(), project_chains.clone(), project_runtime.clone(),
                                saved_project_snapshot.clone(), project_dirty.clone(),
                                input_chain_devices.clone(), output_chain_devices.clone(),
                                "block-window.number",
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
                            );
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
                            &input_chain_devices, &output_chain_devices, true,
                        ) {
                            log::error!("[adapter-gui] block-window.save: {e}");
                            main.set_block_drawer_status_message(e.to_string().into());
                            return;
                        }
                        *selected_block.borrow_mut() = None;
                        set_selected_block(&main, None);
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
                        replace_project_chains(&project_chains, &session.project, &input_chain_devices, &output_chain_devices);
                        sync_project_dirty(&main, session, &saved_project_snapshot, &project_dirty);
                        drop(session_borrow);
                        *selected_block.borrow_mut() = None;
                        set_selected_block(&main, None);
                        open_block_windows.borrow_mut().retain(|bw| {
                            bw.chain_index != draft.chain_index || bw.block_index != block_index
                        });
                        let _ = win.hide();
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
                        set_selected_block(&main, None);
                        let _ = win.hide();
                    });
                }
                let _ = win.show();
                open_block_windows.borrow_mut().push(BlockWindow { chain_index: ci, block_index: bi, window: win });
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
            set_selected_block(&window, None);
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
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        window.on_toggle_chain_block_enabled(move |chain_index, block_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            log::info!("on_toggle_chain_block_enabled: chain_index={}, block_index={}", chain_index, block_index);
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
                return;
            };
            let Some(chain) = session.project.chains.get_mut(chain_index as usize) else {
                set_status_error(&window, &toast_timer, "Chain inválida.");
                return;
            };
            let Some(block) = chain.blocks.get_mut(block_index as usize) else {
                set_status_error(&window, &toast_timer, "Block inválido.");
                return;
            };
            block.enabled = !block.enabled;
            let chain_id = chain.id.clone();
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&window, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
                &input_chain_devices,
                &output_chain_devices,
            );
            *selected_block.borrow_mut() = Some(SelectedBlock {
                chain_index: chain_index as usize,
                block_index: block_index as usize,
            });
            set_selected_block(&window, selected_block.borrow().as_ref());
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
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
        window.on_reorder_chain_block(move |chain_index, from_index, before_index| {
            log::info!("[reorder_chain_block] chain_index={}, from_index={}, before_index={}", chain_index, from_index, before_index);
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
                let block_count = chain.blocks.len() as i32;
                if from_index < 0 || from_index >= block_count {
                    return;
                }
                let mut normalized_before = before_index.clamp(0, block_count);
                if normalized_before == from_index || normalized_before == from_index + 1 {
                    return;
                }
                let block = chain.blocks.remove(from_index as usize);
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
                &input_chain_devices,
                &output_chain_devices,
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
            set_selected_block(&window, None);
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
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
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = Some(BlockEditorDraft {
                chain_index: chain_index as usize,
                block_index: None,
                before_index: before_index as usize,
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
            set_selected_block(&window, None);
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
        let weak_block_editor_window = block_editor_window.as_weak();
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
                window.set_show_block_drawer(true);
            } else {
                window.set_show_block_drawer(false);
                if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                    block_editor_window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
                    sync_block_editor_window(&window, &block_editor_window);
                    let _ = block_editor_window.show();
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
            window.set_block_drawer_selected_model_index(index);
            window.set_block_drawer_status_message("".into());
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
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
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        window.on_close_block_drawer(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            block_editor_persist_timer.stop();
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = None;
            block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
            window.set_block_drawer_selected_model_index(-1);
            window.set_block_drawer_selected_type_index(-1);
            set_selected_block(&window, None);
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
                    );
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
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_persist_timer = block_editor_persist_timer.clone();
        let weak_block_editor_window = block_editor_window.as_weak();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
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
                &input_chain_devices,
                &output_chain_devices,
                true,
            ) {
                log::error!("[adapter-gui] block-drawer.save: {error}");
                window.set_block_drawer_status_message(error.to_string().into());
                return;
            }
            *selected_block.borrow_mut() = None;
            set_selected_block(&window, None);
            *block_editor_draft.borrow_mut() = None;
            block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
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
                &input_chain_devices,
                &output_chain_devices,
            );
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            *selected_block.borrow_mut() = None;
            *block_editor_draft.borrow_mut() = None;
            block_model_options.set_vec(Vec::new());
            block_model_option_labels.set_vec(Vec::new());
            block_parameter_items.set_vec(Vec::new());
            set_selected_block(&window, None);
            window.set_show_block_drawer(false);
            window.set_block_drawer_status_message("".into());
            clear_status(&window, &toast_timer);
            if let Some(block_editor_window) = weak_block_editor_window.upgrade() {
                let _ = block_editor_window.hide();
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
            if draft.input_device_id.is_none() {
                set_status_warning(&window, &toast_timer, "Selecione o dispositivo de entrada.");
                return;
            }
            if draft.output_device_id.is_none() {
                set_status_warning(&window, &toast_timer, "Selecione o dispositivo de saída.");
                return;
            }
            if draft.input_channels.is_empty() {
                set_status_warning(&window, &toast_timer, "Selecione pelo menos um canal de entrada.");
                return;
            }
            if draft.output_channels.is_empty() {
                set_status_warning(&window, &toast_timer, "Selecione pelo menos um canal de saída.");
                return;
            }
            let editing_index = draft.editing_index;
            log::debug!("[save_chain] editing_index={:?}, draft.instrument='{}'", editing_index, draft.instrument);
            let existing_chain =
                editing_index.and_then(|index| session.project.chains.get(index).cloned());
            let chain = Chain {
                id: existing_chain
                    .as_ref()
                    .map(|chain| chain.id.clone())
                    .unwrap_or_else(ChainId::generate),
                description: normalized_chain_description(&draft.name),
                instrument: draft.instrument.clone(),
                enabled: existing_chain
                    .as_ref()
                    .map(|chain| chain.enabled)
                    .unwrap_or(false),
                input_device_id: DeviceId(draft.input_device_id.unwrap_or_default()),
                input_channels: draft.input_channels,
                output_device_id: DeviceId(draft.output_device_id.unwrap_or_default()),
                output_channels: draft.output_channels,
                blocks: existing_chain
                    .as_ref()
                    .map(|chain| chain.blocks.clone())
                    .unwrap_or_default(),
                output_mixdown: existing_chain
                    .as_ref()
                    .map(|chain| chain.output_mixdown)
                    .unwrap_or(ChainOutputMixdown::Average),
                input_mode: existing_chain
                    .as_ref()
                    .map(|chain| chain.input_mode)
                    .unwrap_or(ChainInputMode::Auto),
            };
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
                &input_chain_devices,
                &output_chain_devices,
            );
            *chain_draft.borrow_mut() = None;
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let toast_timer = toast_timer.clone();
        chain_editor_window.on_save_chain(move || {
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
            if draft.input_device_id.is_none() {
                chain_window.set_status_message("Selecione o dispositivo de entrada.".into());
                return;
            }
            if draft.output_device_id.is_none() {
                chain_window.set_status_message("Selecione o dispositivo de saída.".into());
                return;
            }
            if draft.input_channels.is_empty() {
                chain_window.set_status_message("Selecione pelo menos um canal de entrada.".into());
                return;
            }
            if draft.output_channels.is_empty() {
                chain_window.set_status_message("Selecione pelo menos um canal de saída.".into());
                return;
            }
            let editing_index = draft.editing_index;
            log::debug!("[save_chain] editing_index={:?}, draft.instrument='{}'", editing_index, draft.instrument);
            let existing_chain =
                editing_index.and_then(|index| session.project.chains.get(index).cloned());
            let chain = Chain {
                id: existing_chain
                    .as_ref()
                    .map(|chain| chain.id.clone())
                    .unwrap_or_else(ChainId::generate),
                description: normalized_chain_description(&draft.name),
                instrument: draft.instrument.clone(),
                enabled: existing_chain
                    .as_ref()
                    .map(|chain| chain.enabled)
                    .unwrap_or(false),
                input_device_id: DeviceId(draft.input_device_id.unwrap_or_default()),
                input_channels: draft.input_channels,
                output_device_id: DeviceId(draft.output_device_id.unwrap_or_default()),
                output_channels: draft.output_channels,
                blocks: existing_chain
                    .as_ref()
                    .map(|chain| chain.blocks.clone())
                    .unwrap_or_default(),
                output_mixdown: existing_chain
                    .as_ref()
                    .map(|chain| chain.output_mixdown)
                    .unwrap_or(ChainOutputMixdown::Average),
                input_mode: existing_chain
                    .as_ref()
                    .map(|chain| chain.input_mode)
                    .unwrap_or(ChainInputMode::Auto),
            };
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
                &input_chain_devices,
                &output_chain_devices,
            );
            *chain_draft.borrow_mut() = None;
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            chain_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(false);
            let _ = chain_window.hide();
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
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let toast_timer = toast_timer.clone();
        chain_editor_window.on_cancel_chain(move || {
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
    {
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
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
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                let _ = input_window.hide();
                return;
            };
            if draft.input_device_id.is_none() || draft.input_channels.is_empty() {
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
                chain.input_device_id = DeviceId(draft.input_device_id.clone().unwrap_or_default());
                chain.input_channels = draft.input_channels.clone();
                let chain_id = chain.id.clone();
                if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                    eprintln!("input editor save error: {error}");
                    return;
                }
                replace_project_chains(
                    &project_chains,
                    &session.project,
                    &input_chain_devices,
                    &output_chain_devices,
                );
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            }
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_endpoint_summaries(
                    &window,
                    &chain_window,
                    draft,
                    &input_chain_devices,
                    &output_chain_devices,
                );
            }
            input_window.set_status_message("".into());
            let _ = input_window.hide();
        });
    }
    {
        let weak_output_window = chain_output_window.as_weak();
        chain_output_window.on_cancel(move || {
            if let Some(output_window) = weak_output_window.upgrade() {
                output_window.set_status_message("".into());
                let _ = output_window.hide();
            }
        });
    }
    {
        let weak_input_window = chain_input_window.as_weak();
        chain_input_window.on_cancel(move || {
            if let Some(input_window) = weak_input_window.upgrade() {
                input_window.set_status_message("".into());
                let _ = input_window.hide();
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let weak_chain_window = chain_editor_window.as_weak();
        let chain_draft = chain_draft.clone();
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
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                let _ = output_window.hide();
                return;
            };
            if draft.output_device_id.is_none() || draft.output_channels.is_empty() {
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
                chain.output_device_id = DeviceId(draft.output_device_id.clone().unwrap_or_default());
                chain.output_channels = draft.output_channels.clone();
                let chain_id = chain.id.clone();
                if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                    eprintln!("output editor save error: {error}");
                    return;
                }
                replace_project_chains(
                    &project_chains,
                    &session.project,
                    &input_chain_devices,
                    &output_chain_devices,
                );
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            }
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_endpoint_summaries(
                    &window,
                    &chain_window,
                    draft,
                    &input_chain_devices,
                    &output_chain_devices,
                );
            }
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
                &input_chain_devices,
                &output_chain_devices,
            );
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
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
                let input_id = chain.input_device_id.clone();
                let input_channels = chain.input_channels.clone();
                let chain_id = chain.id.clone();
                for other in &session.project.chains {
                    if other.id != chain_id && other.enabled
                        && other.input_device_id == input_id
                        && other.input_channels.iter().any(|ch| input_channels.contains(ch))
                    {
                        let other_name = other.description.as_deref().unwrap_or("outra chain");
                        set_status_error(&window, &toast_timer, &format!("Input channel já em uso por '{}'", other_name));
                        return;
                    }
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
                &input_chain_devices,
                &output_chain_devices,
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
        if !path.exists() {
            continue;
        }
        let canonical_path = canonical_project_path(&path).unwrap_or(path.clone());
        let canonical_path_string = canonical_path.to_string_lossy().to_string();
        if synced
            .iter()
            .any(|current: &RecentProjectEntry| current.project_path == canonical_path_string)
        {
            continue;
        }
        match (YamlProjectRepository {
            path: canonical_path.clone(),
        })
        .load_current_project()
        {
            Ok(project) => synced.push(RecentProjectEntry {
                project_path: canonical_path_string,
                project_name: project_display_name(&project),
                is_valid: true,
                invalid_reason: None,
            }),
            Err(_) => synced.push(RecentProjectEntry {
                project_path: canonical_path_string,
                project_name: if recent.project_name.trim().is_empty() {
                    UNTITLED_PROJECT_NAME.to_string()
                } else {
                    recent.project_name.clone()
                },
                is_valid: false,
                invalid_reason: Some("Projeto inválido".to_string()),
            }),
        }
    }
    config.recent_projects = synced;
    *config != original
}
fn canonical_project_path(path: &PathBuf) -> Result<PathBuf> {
    if path.exists() {
        return Ok(fs::canonicalize(path)?);
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
    let project = YamlProjectRepository {
        path: project_path.to_path_buf(),
    }
    .load_current_project()?;
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
            let input_settings = project
                .device_settings
                .iter()
                .find(|s| s.device_id == chain.input_device_id);
            let output_settings = project
                .device_settings
                .iter()
                .find(|s| s.device_id == chain.output_device_id);
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
                block_count_label: if chain.blocks.len() == 1 {
                    "1 block".into()
                } else {
                    format!("{} blocks", chain.blocks.len()).into()
                },
                input_label: chain_endpoint_label("In", &chain.input_channels).into(),
                input_tooltip: chain_endpoint_tooltip(
                    "Entrada",
                    &chain.input_device_id.0,
                    &chain.input_channels,
                    project,
                    input_devices,
                )
                .into(),
                output_label: chain_endpoint_label("Out", &chain.output_channels).into(),
                output_tooltip: chain_endpoint_tooltip(
                    "Saída",
                    &chain.output_device_id.0,
                    &chain.output_channels,
                    project,
                    output_devices,
                )
                .into(),
                latency_ms,
                blocks: ModelRc::from(Rc::new(VecModel::from(
                    chain
                        .blocks
                        .iter()
                        .map(chain_block_item_from_block)
                        .collect::<Vec<_>>(),
                ))),
            }
        })
        .collect::<Vec<_>>();
    model.set_vec(items);
}
fn chain_endpoint_label(prefix: &str, _channels: &[usize]) -> String {
    prefix.to_string()
}
fn chain_endpoint_tooltip(
    _title: &str,
    device_id: &str,
    channels: &[usize],
    project: &Project,
    devices: &[AudioDeviceDescriptor],
) -> String {
    let device_name = devices
        .iter()
        .find(|device| device.id == device_id)
        .map(|device| device.name.as_str())
        .unwrap_or(device_id);
    let settings = project
        .device_settings
        .iter()
        .find(|setting| setting.device_id.0 == device_id);
    let sample_rate = settings
        .map(|setting| setting.sample_rate.to_string())
        .unwrap_or_else(|| "n/d".to_string());
    let buffer = settings
        .map(|setting| setting.buffer_size_frames.to_string())
        .unwrap_or_else(|| "n/d".to_string());
    format!(
        "{device_name}\nConfiguração: {sample_rate} Hz · {buffer} frames\nCanais: {}",
        format_channel_list(channels)
    )
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
    supported_block_types()
        .into_iter()
        .filter(|item| seen.insert(item.effect_type))
        .map(|item| BlockTypePickerItem {
            effect_type: item.effect_type.into(),
            label: item.display_label.into(),
            subtitle: "".into(),
            icon_kind: item.icon_kind.into(),
            use_panel_editor: item.use_panel_editor,
        })
        .filter(|item| {
            instrument == block_core::INST_GENERIC || !block_model_picker_items(item.effect_type.as_str(), instrument).is_empty()
        })
        .collect()
}
fn block_model_picker_items(effect_type: &str, instrument: &str) -> Vec<BlockModelPickerItem> {
    let all_models = supported_block_models(effect_type).unwrap_or_default();
    log::debug!("[block_model_picker_items] effect_type='{}', instrument='{}', total_models={}", effect_type, instrument, all_models.len());
    for m in &all_models {
        log::debug!("[block_model_picker_items]   model='{}' supported_instruments={:?}", m.model_id, m.supported_instruments);
    }
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
fn set_selected_block(window: &AppWindow, selected_block: Option<&SelectedBlock>) {
    if let Some(selected_block) = selected_block {
        window.set_selected_chain_block_chain_index(selected_block.chain_index as i32);
        window.set_selected_chain_block_index(selected_block.block_index as i32);
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
                label: spec.label.clone().into(),
                group: spec.group.clone().unwrap_or_default().into(),
                widget_kind: match &spec.domain {
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
) -> Result<()> {
    let params =
        block_parameter_values(block_parameter_items, &draft.effect_type, &draft.model_id)?;
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
            chain.blocks.insert(
                insert_index,
                AudioBlock {
                    id: BlockId::generate_for_chain(&chain.id),
                    enabled: draft.enabled,
                    kind,
                },
            );
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
    sync_project_dirty(window, session, saved_project_snapshot, project_dirty);
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
        if rows
            .iter()
            .any(|row| row.device_id.as_str() == device.id.as_str())
        {
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
        });
    }
    rows
}
fn build_project_yaml(session: &ProjectSession) -> Result<ProjectYaml> {
    Ok(ProjectYaml {
        name: session
            .project
            .name
            .as_ref()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty()),
        device_settings: session
            .project
            .device_settings
            .iter()
            .map(|setting| ProjectDeviceSettingsYaml {
                device_id: setting.device_id.0.clone(),
                sample_rate: setting.sample_rate,
                buffer_size_frames: setting.buffer_size_frames,
            })
            .collect(),
        chains: session
            .project
            .chains
            .iter()
            .map(|chain| -> Result<ProjectChainYaml> {
                Ok(ProjectChainYaml {
                    description: chain.description.clone(),
                    instrument: chain.instrument.clone(),
                    input_device_id: chain.input_device_id.0.clone(),
                    input_channels: chain.input_channels.clone(),
                    output_device_id: chain.output_device_id.0.clone(),
                    output_channels: chain.output_channels.clone(),
                    blocks: serialize_audio_blocks(&chain.blocks)?,
                    output_mixdown: chain.output_mixdown,
                    input_mode: chain.input_mode,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    })
}
fn project_session_snapshot(session: &ProjectSession) -> Result<String> {
    Ok(serde_yaml::to_string(&build_project_yaml(session)?)?)
}
fn set_project_dirty(window: &AppWindow, project_dirty: &Rc<RefCell<bool>>, dirty: bool) {
    *project_dirty.borrow_mut() = dirty;
    window.set_project_dirty(dirty);
}
fn sync_project_dirty(
    window: &AppWindow,
    session: &ProjectSession,
    saved_project_snapshot: &Rc<RefCell<Option<String>>>,
    project_dirty: &Rc<RefCell<bool>>,
) {
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
    ChainDraft {
        editing_index: None,
        name: format!("Chain {}", project.chains.len() + 1),
        instrument: block_core::DEFAULT_INSTRUMENT.to_string(),
        input_device_id: input_devices.first().map(|device| device.id.clone()),
        output_device_id: output_devices.first().map(|device| device.id.clone()),
        input_channels: Vec::new(),
        output_channels: Vec::new(),
    }
}
fn chain_draft_from_chain(index: usize, chain: &Chain) -> ChainDraft {
    ChainDraft {
        editing_index: Some(index),
        name: chain
            .description
            .clone()
            .unwrap_or_else(|| format!("Chain {}", index + 1)),
        instrument: chain.instrument.clone(),
        input_device_id: Some(chain.input_device_id.0.clone()),
        output_device_id: Some(chain.output_device_id.0.clone()),
        input_channels: chain.input_channels.clone(),
        output_channels: chain.output_channels.clone(),
    }
}
fn chain_block_item_from_block(block: &AudioBlock) -> ChainBlockItem {
    let (kind, label) = match &block.kind {
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
    ChainBlockItem {
        kind: kind.into(),
        icon_kind: block_type
            .as_ref()
            .map(|entry| entry.icon_kind)
            .unwrap_or("core")
            .into(),
        type_label: block_type
            .as_ref()
            .map(|entry| entry.display_label)
            .unwrap_or("BLOCK")
            .into(),
        label: label.into(),
        family: family.into(),
        enabled: block.enabled,
    }
}
fn build_input_channel_items(
    draft: &ChainDraft,
    project: &Project,
    input_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = draft.input_device_id.as_ref() else {
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
                && chain.input_device_id.0 == *device_id
                && draft.editing_index != Some(*index)
        })
        .flat_map(|(_, chain)| chain.input_channels.iter().copied())
        .collect::<Vec<_>>();
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: format!("Canal {}", channel + 1).into(),
            selected: draft.input_channels.contains(&channel),
            available: !used_channels.contains(&channel),
        })
        .collect()
}
fn build_output_channel_items(
    draft: &ChainDraft,
    output_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = draft.output_device_id.as_ref() else {
        return Vec::new();
    };
    let Some(device) = output_devices.iter().find(|device| &device.id == device_id) else {
        return Vec::new();
    };
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: format!("Canal {}", channel + 1).into(),
            selected: draft.output_channels.contains(&channel),
            available: true,
        })
        .collect()
}
fn replace_channel_options(model: &Rc<VecModel<ChannelOptionItem>>, items: Vec<ChannelOptionItem>) {
    model.set_vec(items);
}
fn normalized_chain_description(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
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
    format!("{device_name}\nCanais: {channels}")
}
fn apply_chain_endpoint_summaries(
    window: &AppWindow,
    chain_editor_window: &ChainEditorWindow,
    draft: &ChainDraft,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) {
    let input_summary = endpoint_summary(
        draft.input_device_id.as_deref(),
        &draft.input_channels,
        input_devices,
    );
    let output_summary = endpoint_summary(
        draft.output_device_id.as_deref(),
        &draft.output_channels,
        output_devices,
    );
    window.set_chain_input_summary(input_summary.clone().into());
    window.set_chain_output_summary(output_summary.clone().into());
    chain_editor_window.set_input_summary(input_summary.into());
    chain_editor_window.set_output_summary(output_summary.into());
}
fn apply_chain_input_window_state(
    input_window: &ChainInputWindow,
    draft: &ChainDraft,
    project: &Project,
    input_devices: &[AudioDeviceDescriptor],
    channel_model: &Rc<VecModel<ChannelOptionItem>>,
) {
    replace_channel_options(
        channel_model,
        build_input_channel_items(draft, project, input_devices),
    );
    input_window.set_selected_device_index(selected_device_index(
        input_devices,
        draft.input_device_id.as_deref(),
    ));
    input_window.set_status_message("".into());
}
fn apply_chain_output_window_state(
    output_window: &ChainOutputWindow,
    draft: &ChainDraft,
    output_devices: &[AudioDeviceDescriptor],
    channel_model: &Rc<VecModel<ChannelOptionItem>>,
) {
    replace_channel_options(
        channel_model,
        build_output_channel_items(draft, output_devices),
    );
    output_window.set_selected_device_index(selected_device_index(
        output_devices,
        draft.output_device_id.as_deref(),
    ));
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
    }
}
fn normalize_device_settings(mut settings: GuiAudioDeviceSettings) -> GuiAudioDeviceSettings {
    if !SUPPORTED_SAMPLE_RATES.contains(&settings.sample_rate) {
        settings.sample_rate = DEFAULT_SAMPLE_RATE;
    }
    if !SUPPORTED_BUFFER_SIZES.contains(&settings.buffer_size_frames) {
        settings.buffer_size_frames = DEFAULT_BUFFER_SIZE_FRAMES;
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
        quantize_numeric_value, SELECT_SELECTED_BLOCK_ID,
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
}