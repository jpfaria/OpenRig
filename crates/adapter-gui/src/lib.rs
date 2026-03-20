use anyhow::{anyhow, Result};
use application::validate::validate_project;
use cpal::{traits::StreamTrait, Stream};
use domain::ids::{BlockId, DeviceId, TrackId};
use engine::runtime::build_runtime_graph;
use infra_cpal::{
    build_streams_for_project, list_input_device_descriptors, list_output_device_descriptors,
    resolve_project_track_sample_rates, AudioDeviceDescriptor,
};
use infra_filesystem::{
    AppConfig, FilesystemStorage, GuiAudioDeviceSettings, GuiAudioSettings, RecentProjectEntry,
};
use infra_yaml::{
    load_track_preset_file, save_track_preset_file, serialize_audio_blocks, TrackBlocksPreset,
    YamlProjectRepository,
};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use std::{cell::RefCell, env, fs, path::PathBuf};
use project::device::DeviceSettings;
use project::block::{
    schema_for_block_model, AmpComboBlock, AmpHeadBlock, AudioBlock, AudioBlockKind,
    CabBlock,
    CompressorBlock, CoreBlock, CoreBlockKind, DelayBlock, DriveBlock, EqBlock, FullRigBlock,
    GateBlock, NamBlock, ReverbBlock, TremoloBlock, TunerBlock,
};
use project::param::{ParameterDomain, ParameterSet, ParameterUnit};
use project::project::Project;
use project::track::{Track, TrackOutputMixdown};
use ui_openrig::{AppRuntimeMode, InteractionMode, UiRuntimeContext};
use ui_state::{
    stage_drawer_state, stage_family_for_kind, stage_models_for_type, stage_types,
    track_routing_summary,
};

mod ui_state;

slint::include_modules!();

const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const DEFAULT_BUFFER_SIZE_FRAMES: u32 = 256;
const SUPPORTED_SAMPLE_RATES: &[u32] = &[44_100, 48_000, 88_200, 96_000];
const SUPPORTED_BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024];

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
struct TrackDraft {
    editing_index: Option<usize>,
    name: String,
    input_device_id: Option<String>,
    output_device_id: Option<String>,
    input_channels: Vec<usize>,
    output_channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedStage {
    track_index: usize,
    stage_index: usize,
}

#[derive(Debug, Clone)]
struct StageEditorDraft {
    track_index: usize,
    stage_index: Option<usize>,
    before_index: usize,
    effect_type: String,
    model_id: String,
    enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrackEditorMode {
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
    tracks: Vec<ProjectTrackYaml>,
}

#[derive(Debug, Serialize)]
struct ProjectDeviceSettingsYaml {
    device_id: String,
    sample_rate: u32,
    buffer_size_frames: u32,
}

#[derive(Debug, Serialize)]
struct ProjectTrackYaml {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    enabled: bool,
    input_device_id: String,
    input_channels: Vec<usize>,
    output_device_id: String,
    output_channels: Vec<usize>,
    stages: Vec<serde_yaml::Value>,
    output_mixdown: TrackOutputMixdown,
}

#[derive(Debug, Serialize)]
struct ConfigYaml {
    presets_path: String,
}

const UNTITLED_PROJECT_NAME: &str = "UNTITLED PROJECT";

pub fn run_desktop_app(runtime_mode: AppRuntimeMode, interaction_mode: InteractionMode) -> Result<()> {
    let context = UiRuntimeContext::new(runtime_mode, interaction_mode);
    let settings = FilesystemStorage::load_gui_audio_settings()?.unwrap_or_default();
    let needs_audio_settings = context.capabilities.can_select_audio_device && !settings.is_complete();
    let project_paths = resolve_project_paths();
    let app_config = Rc::new(RefCell::new(load_and_sync_app_config()?));
    let project_session = Rc::new(RefCell::new(None::<ProjectSession>));
    let track_draft = Rc::new(RefCell::new(None::<TrackDraft>));
    let selected_stage = Rc::new(RefCell::new(None::<SelectedStage>));
    let stage_editor_draft = Rc::new(RefCell::new(None::<StageEditorDraft>));
    let project_streams = Rc::new(RefCell::new(None::<Vec<Stream>>));
    let saved_project_snapshot = Rc::new(RefCell::new(None::<String>));
    let project_dirty = Rc::new(RefCell::new(false));
    let audio_settings_mode = Rc::new(RefCell::new(AudioSettingsMode::Gui));
    let input_track_devices = Rc::new(list_input_device_descriptors()?);
    let output_track_devices = Rc::new(list_output_device_descriptors()?);

    let window = AppWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let project_settings_window =
        ProjectSettingsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let track_editor_window =
        TrackEditorWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    window.set_show_project_launcher(true);
    window.set_show_project_tracks(false);
    window.set_show_track_editor(false);
    window.set_show_project_settings(false);
    window.set_project_running(false);
    window.set_project_dirty(false);
    window.set_project_path_label("".into());
    window.set_project_title("Projeto".into());
    window.set_project_name_draft("".into());
    window.set_recent_project_search("".into());
    window.set_track_editor_title("Nova track".into());
    window.set_track_editor_save_label("Criar track".into());
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

    window.set_input_devices(ModelRc::from(input_devices.clone()));
    window.set_output_devices(ModelRc::from(output_devices.clone()));
    let project_tracks = Rc::new(VecModel::from(Vec::<ProjectTrackItem>::new()));
    window.set_project_tracks(ModelRc::from(project_tracks.clone()));
    let recent_projects = Rc::new(VecModel::from(recent_project_items(
        &app_config.borrow().recent_projects,
        "",
    )));
    window.set_recent_projects(ModelRc::from(recent_projects.clone()));
    let track_input_device_options = Rc::new(VecModel::from(
        input_track_devices
            .iter()
            .map(|device| SharedString::from(device.name.clone()))
            .collect::<Vec<_>>(),
    ));
    let track_output_device_options = Rc::new(VecModel::from(
        output_track_devices
            .iter()
            .map(|device| SharedString::from(device.name.clone()))
            .collect::<Vec<_>>(),
    ));
    let track_input_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    let track_output_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    window.set_track_input_device_options(ModelRc::from(track_input_device_options.clone()));
    window.set_track_output_device_options(ModelRc::from(track_output_device_options.clone()));
    window.set_track_input_channels(ModelRc::from(track_input_channels.clone()));
    window.set_track_output_channels(ModelRc::from(track_output_channels.clone()));
    window.set_selected_track_input_device_index(-1);
    window.set_selected_track_output_device_index(-1);
    window.set_selected_track_stage_track_index(-1);
    window.set_selected_track_stage_index(-1);
    window.set_show_stage_type_picker(false);
    window.set_show_stage_model_picker(false);
    window.set_stage_picker_title("Adicionar stage".into());
    window.set_show_stage_drawer(false);
    window.set_stage_drawer_title("Adicionar stage".into());
    window.set_stage_drawer_confirm_label("Adicionar".into());
    window.set_stage_drawer_status_message("".into());
    window.set_stage_drawer_edit_mode(false);
    window.set_stage_drawer_selected_type_index(-1);
    window.set_stage_drawer_selected_model_index(-1);
    window.set_stage_drawer_enabled(true);
    window.set_track_draft_name("".into());
    project_settings_window.set_status_message("".into());
    track_editor_window.set_status_message("".into());
    project_settings_window.set_project_name_draft("".into());
    track_editor_window.set_track_name("".into());

    let stage_type_options = Rc::new(VecModel::from(stage_type_picker_items()));
    let stage_model_options = Rc::new(VecModel::from(Vec::<StageModelPickerItem>::new()));
    let stage_parameter_items = Rc::new(VecModel::from(Vec::<StageParameterItem>::new()));
    window.set_stage_type_options(ModelRc::from(stage_type_options.clone()));
    window.set_stage_model_options(ModelRc::from(stage_model_options.clone()));
    window.set_stage_parameter_items(ModelRc::from(stage_parameter_items.clone()));

    project_settings_window.set_input_devices(ModelRc::from(input_devices.clone()));
    project_settings_window.set_output_devices(ModelRc::from(output_devices.clone()));
    project_settings_window.set_sample_rate_options(window.get_sample_rate_options());
    project_settings_window.set_buffer_size_options(window.get_buffer_size_options());

    track_editor_window.set_input_device_options(ModelRc::from(track_input_device_options.clone()));
    track_editor_window.set_output_device_options(ModelRc::from(track_output_device_options.clone()));
    track_editor_window.set_input_channels(ModelRc::from(track_input_channels.clone()));
    track_editor_window.set_output_channels(ModelRc::from(track_output_channels.clone()));
    track_editor_window.set_selected_input_device_index(-1);
    track_editor_window.set_selected_output_device_index(-1);

    {
        let input_devices = input_devices.clone();
        window.on_toggle_input_device(move |index, selected| {
            toggle_device_row(&input_devices, index as usize, selected);
        });
    }

    {
        let input_devices = input_devices.clone();
        project_settings_window.on_toggle_input_device(move |index, selected| {
            toggle_device_row(&input_devices, index as usize, selected);
        });
    }

    {
        let output_devices = output_devices.clone();
        window.on_toggle_output_device(move |index, selected| {
            toggle_device_row(&output_devices, index as usize, selected);
        });
    }

    {
        let output_devices = output_devices.clone();
        project_settings_window.on_toggle_output_device(move |index, selected| {
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
        project_settings_window.on_update_input_sample_rate(move |index, value| {
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
        let input_devices = input_devices.clone();
        project_settings_window.on_update_input_buffer_size(move |index, value| {
            update_device_buffer_size(&input_devices, index as usize, value);
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
        project_settings_window.on_update_output_sample_rate(move |index, value| {
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
        let output_devices = output_devices.clone();
        project_settings_window.on_update_output_buffer_size(move |index, value| {
            update_device_buffer_size(&output_devices, index as usize, value);
        });
    }

    {
        let weak_window = window.as_weak();
        let input_devices = input_devices.clone();
        window.on_go_to_output_step(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            match selected_device_settings(&input_devices, "input") {
                Ok(devices) if !devices.is_empty() => {
                    window.set_status_message("".into());
                    window.set_wizard_step(1);
                }
                Ok(_) => {
                    window.set_status_message(
                        "Selecione pelo menos um input antes de continuar.".into(),
                    );
                }
                Err(error) => {
                    window.set_status_message(error.to_string().into());
                }
            }
        });
    }

    {
        let weak_window = window.as_weak();
        window.on_go_to_input_step(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            window.set_status_message("".into());
            window.set_wizard_step(0);
        });
    }

    {
        let weak_window = window.as_weak();
        let input_devices = input_devices.clone();
        let output_devices = output_devices.clone();
        let audio_settings_mode = audio_settings_mode.clone();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_save_audio_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let input_devices = match selected_device_settings(&input_devices, "input") {
                Ok(devices) => devices,
                Err(error) => {
                    window.set_status_message(error.to_string().into());
                    return;
                }
            };
            let output_devices = match selected_device_settings(&output_devices, "output") {
                Ok(devices) => devices,
                Err(error) => {
                    window.set_status_message(error.to_string().into());
                    return;
                }
            };

            let settings = GuiAudioSettings {
                input_devices,
                output_devices,
            };

            if !settings.is_complete() {
                window.set_status_message(
                    "Selecione pelo menos um input e um output antes de continuar.".into(),
                );
                return;
            }

            match *audio_settings_mode.borrow() {
                AudioSettingsMode::Gui => match FilesystemStorage::save_gui_audio_settings(&settings) {
                    Ok(()) => {
                        window.set_status_message("".into());
                        window.set_show_audio_settings(false);
                    }
                    Err(error) => window.set_status_message(error.to_string().into()),
                },
                AudioSettingsMode::Project => {
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        window.set_status_message("Nenhum projeto carregado.".into());
                        return;
                    };
                    session.project.device_settings = merge_device_settings(
                        settings.input_devices,
                        settings.output_devices,
                    );
                    let was_running = project_streams.borrow().is_some();
                    if was_running {
                        stop_project_runtime(&project_streams);
                        window.set_project_running(false);
                    }
                    replace_project_tracks(&project_tracks, &session.project);
                    window.set_project_title(
                        project_title_for_path(session.project_path.as_ref(), &session.project).into(),
                    );
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
                    window.set_status_message("".into());
                    window.set_show_project_tracks(true);
                    window.set_show_track_editor(false);
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
        let audio_settings_mode = audio_settings_mode.clone();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        project_settings_window.on_save_audio_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = weak_settings.upgrade() else {
                return;
            };
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

            match *audio_settings_mode.borrow() {
                AudioSettingsMode::Gui => match FilesystemStorage::save_gui_audio_settings(&settings) {
                    Ok(()) => {
                        settings_window.set_status_message("".into());
                        window.set_status_message("".into());
                        window.set_show_audio_settings(false);
                        let _ = settings_window.hide();
                    }
                    Err(error) => settings_window.set_status_message(error.to_string().into()),
                },
                AudioSettingsMode::Project => {
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        settings_window.set_status_message("Nenhum projeto carregado.".into());
                        return;
                    };
                    session.project.device_settings = merge_device_settings(
                        settings.input_devices,
                        settings.output_devices,
                    );
                    let was_running = project_streams.borrow().is_some();
                    if was_running {
                        stop_project_runtime(&project_streams);
                        window.set_project_running(false);
                    }
                    replace_project_tracks(&project_tracks, &session.project);
                    window.set_project_title(
                        project_title_for_path(session.project_path.as_ref(), &session.project).into(),
                    );
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
                    settings_window.set_status_message("".into());
                    window.set_status_message("".into());
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
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let recent_projects = recent_projects.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_open_project_file(move || {
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
            match load_project_session(&path, &resolve_project_config_path(&path)) {
                Ok(session) => {
                    let canonical_path = canonical_project_path(&path).unwrap_or(path.clone());
                    let title = project_title_for_path(Some(&canonical_path), &session.project);
                    let display_name = project_display_name(&session.project);
                    stop_project_runtime(&project_streams);
                    replace_project_tracks(&project_tracks, &session.project);
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
                    window.set_project_running(false);
                    set_project_dirty(&window, &project_dirty, false);
                    window.set_status_message("".into());
                    window.set_project_title(title.into());
                    window.set_project_name_draft(
                        project_session
                            .borrow()
                            .as_ref()
                            .and_then(|session| session.project.name.clone())
                            .unwrap_or_default()
                            .into(),
                    );
                    window.set_project_path_label(format!("Projeto: {}", canonical_path.display()).into());
                    window.set_show_project_launcher(false);
                    window.set_show_project_tracks(true);
                    window.set_show_track_editor(false);
                    window.set_show_project_settings(false);
                }
                Err(error) => {
                    window.set_status_message(error.to_string().into());
                }
            }
        });
    }

    {
        let weak_window = window.as_weak();
        let project_paths = project_paths.clone();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_create_project_file(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            stop_project_runtime(&project_streams);
            let session = create_new_project_session(&project_paths.default_config_path);
            replace_project_tracks(&project_tracks, &session.project);
            *project_session.borrow_mut() = Some(session);
            *saved_project_snapshot.borrow_mut() = None;
            window.set_project_running(false);
            window.set_status_message("".into());
            set_project_dirty(&window, &project_dirty, true);
            window.set_project_title("Novo Projeto".into());
            window.set_project_name_draft("".into());
            window.set_project_path_label("Projeto em memória".into());
            window.set_show_project_launcher(false);
            window.set_show_project_tracks(true);
            window.set_show_track_editor(false);
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
        window.on_save_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                window.set_status_message("Nenhum projeto carregado.".into());
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
                    window.set_project_name_draft(session.project.name.clone().unwrap_or_default().into());
                    window.set_project_path_label(format!("Projeto: {}", project_path.display()).into());
                    *saved_project_snapshot.borrow_mut() = project_session_snapshot(session).ok();
                    set_project_dirty(&window, &project_dirty, false);
                    window.set_status_message("".into());
                }
                Err(error) => {
                    window.set_status_message(error.to_string().into());
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
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let recent_projects = recent_projects.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
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
                window.set_status_message("Projeto recente inválido.".into());
                return;
            };
            if !recent.is_valid {
                window.set_status_message(
                    recent
                        .invalid_reason
                        .unwrap_or_else(|| "Projeto inválido.".to_string())
                        .into(),
                );
                return;
            }

            let path = PathBuf::from(&recent.project_path);
            match load_project_session(&path, &resolve_project_config_path(&path)) {
                Ok(session) => {
                    let canonical_path = canonical_project_path(&path).unwrap_or(path.clone());
                    let title = project_title_for_path(Some(&canonical_path), &session.project);
                    let display_name = project_display_name(&session.project);
                    stop_project_runtime(&project_streams);
                    replace_project_tracks(&project_tracks, &session.project);
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
                    window.set_project_running(false);
                    set_project_dirty(&window, &project_dirty, false);
                    window.set_status_message("".into());
                    window.set_project_title(title.into());
                    window.set_project_name_draft(
                        project_session
                            .borrow()
                            .as_ref()
                            .and_then(|session| session.project.name.clone())
                            .unwrap_or_default()
                            .into(),
                    );
                    window.set_project_path_label(format!("Projeto: {}", canonical_path.display()).into());
                    window.set_show_project_launcher(false);
                    window.set_show_project_tracks(true);
                    window.set_show_track_editor(false);
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
                    window.set_status_message("Projeto inválido. Corrija ou remova da lista.".into());
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
        let input_devices = input_devices.clone();
        let output_devices = output_devices.clone();
        let audio_settings_mode = audio_settings_mode.clone();
        let project_settings_window = project_settings_window.as_weak();
        window.on_configure_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = project_settings_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };

            set_device_selection_from_project(&input_devices, &session.project.device_settings);
            set_device_selection_from_project(&output_devices, &session.project.device_settings);
            *audio_settings_mode.borrow_mut() = AudioSettingsMode::Project;
            window.set_project_name_draft(session.project.name.clone().unwrap_or_default().into());
            settings_window.set_project_name_draft(session.project.name.clone().unwrap_or_default().into());
            settings_window.set_status_message("".into());
            window.set_status_message("".into());
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
        window.on_close_project_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            window.set_status_message("".into());
            window.set_show_project_settings(false);
        });
    }

    {
        let weak_window = window.as_weak();
        let weak_settings = project_settings_window.as_weak();
        project_settings_window.on_close_project_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(settings_window) = weak_settings.upgrade() else {
                return;
            };
            settings_window.set_status_message("".into());
            window.set_status_message("".into());
            window.set_show_project_settings(false);
            let _ = settings_window.hide();
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        window.on_save_track_preset(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let Some(track) = session.project.tracks.get(index as usize) else {
                window.set_status_message("Track inválida.".into());
                return;
            };
            let default_name = track
                .description
                .clone()
                .unwrap_or_else(|| format!("track_{}", index + 1))
                .replace(' ', "_")
                .to_lowercase();
            let Some(path) = FileDialog::new()
                .add_filter("OpenRig Preset", &["yaml", "yml"])
                .set_title("Salvar preset")
                .set_directory(&session.presets_path)
                .set_file_name(&format!("{default_name}.yaml"))
                .save_file()
            else {
                return;
            };
            match save_track_blocks_to_preset(track, &path) {
                Ok(()) => window.set_status_message("Preset salvo.".into()),
                Err(error) => window.set_status_message(error.to_string().into()),
            }
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_configure_track_preset(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let Some(path) = FileDialog::new()
                .add_filter("OpenRig Preset", &["yaml", "yml"])
                .set_title("Carregar preset na track")
                .set_directory(&session.presets_path)
                .pick_file()
            else {
                return;
            };
            match load_preset_file(&path) {
                Ok(preset) => {
                    if let Some(track) = session.project.tracks.get_mut(index as usize) {
                        track.blocks = preset.blocks;
                        let was_running = project_streams.borrow().is_some();
                        if was_running {
                            stop_project_runtime(&project_streams);
                            window.set_project_running(false);
                        }
                        replace_project_tracks(&project_tracks, &session.project);
                        sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
                        window.set_status_message("".into());
                    }
                }
                Err(error) => window.set_status_message(error.to_string().into()),
            }
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let project_settings_window = project_settings_window.as_weak();
        let track_editor_window = track_editor_window.as_weak();
        window.on_back_to_launcher(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            if let Some(settings_window) = project_settings_window.upgrade() {
                let _ = settings_window.hide();
            }
            if let Some(editor_window) = track_editor_window.upgrade() {
                let _ = editor_window.hide();
            }
            stop_project_runtime(&project_streams);
            *project_session.borrow_mut() = None;
            *saved_project_snapshot.borrow_mut() = None;
            replace_project_tracks(&project_tracks, &Project {
                name: None,
                device_settings: Vec::new(),
                tracks: Vec::new(),
            });
            window.set_status_message("".into());
            set_project_dirty(&window, &project_dirty, false);
            window.set_project_title("Projeto".into());
            window.set_project_name_draft("".into());
            window.set_project_running(false);
            window.set_project_path_label("".into());
            window.set_show_project_settings(false);
            window.set_show_track_editor(false);
            window.set_show_project_tracks(false);
            window.set_show_project_launcher(true);
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let track_draft = track_draft.clone();
        let input_track_devices = input_track_devices.clone();
        let output_track_devices = output_track_devices.clone();
        let track_input_channels = track_input_channels.clone();
        let track_output_channels = track_output_channels.clone();
        let track_editor_window = track_editor_window.as_weak();
        window.on_add_track(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(editor_window) = track_editor_window.upgrade() else {
                return;
            };
            let borrow = project_session.borrow();
            let Some(session) = borrow.as_ref() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let draft = create_track_draft(&session.project, &input_track_devices, &output_track_devices);
            *track_draft.borrow_mut() = Some(draft.clone());
            apply_track_editor_labels(&window, &draft);
            replace_channel_options(
                &track_input_channels,
                build_input_channel_items(&draft, &session.project, &input_track_devices),
            );
            replace_channel_options(
                &track_output_channels,
                build_output_channel_items(&draft, &output_track_devices),
            );
            window.set_track_draft_name(draft.name.clone().into());
            editor_window.set_track_name(draft.name.clone().into());
            editor_window.set_editor_title(window.get_track_editor_title());
            editor_window.set_editor_save_label(window.get_track_editor_save_label());
            window.set_selected_track_input_device_index(selected_device_index(
                &input_track_devices,
                draft.input_device_id.as_deref(),
            ));
            window.set_selected_track_output_device_index(selected_device_index(
                &output_track_devices,
                draft.output_device_id.as_deref(),
            ));
            editor_window.set_selected_input_device_index(window.get_selected_track_input_device_index());
            editor_window.set_selected_output_device_index(window.get_selected_track_output_device_index());
            editor_window.set_status_message("".into());
            window.set_status_message("".into());
            window.set_show_track_editor(true);
            let _ = editor_window.show();
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let track_draft = track_draft.clone();
        let input_track_devices = input_track_devices.clone();
        let output_track_devices = output_track_devices.clone();
        let track_input_channels = track_input_channels.clone();
        let track_output_channels = track_output_channels.clone();
        let track_editor_window = track_editor_window.as_weak();
        window.on_configure_track(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(editor_window) = track_editor_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let Some(track) = session.project.tracks.get(index as usize) else {
                window.set_status_message("Track inválida.".into());
                return;
            };
            let draft = track_draft_from_track(index as usize, track);
            replace_channel_options(
                &track_input_channels,
                build_input_channel_items(&draft, &session.project, &input_track_devices),
            );
            replace_channel_options(
                &track_output_channels,
                build_output_channel_items(&draft, &output_track_devices),
            );
            window.set_track_draft_name(draft.name.clone().into());
            editor_window.set_track_name(draft.name.clone().into());
            window.set_selected_track_input_device_index(selected_device_index(
                &input_track_devices,
                draft.input_device_id.as_deref(),
            ));
            window.set_selected_track_output_device_index(selected_device_index(
                &output_track_devices,
                draft.output_device_id.as_deref(),
            ));
            editor_window.set_selected_input_device_index(window.get_selected_track_input_device_index());
            editor_window.set_selected_output_device_index(window.get_selected_track_output_device_index());
            *track_draft.borrow_mut() = Some(draft);
            if let Some(draft) = track_draft.borrow().as_ref() {
                apply_track_editor_labels(&window, draft);
                editor_window.set_editor_title(window.get_track_editor_title());
                editor_window.set_editor_save_label(window.get_track_editor_save_label());
            }
            editor_window.set_status_message("".into());
            window.set_status_message("".into());
            window.set_show_track_editor(true);
            let _ = editor_window.show();
        });
    }

    {
        let weak_window = window.as_weak();
        let track_draft = track_draft.clone();
        window.on_update_track_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            if let Some(draft) = track_draft.borrow_mut().as_mut() {
                draft.name = value.to_string();
                window.set_track_draft_name(value);
            }
        });
    }

    {
        let weak_window = window.as_weak();
        let weak_track_window = track_editor_window.as_weak();
        let track_draft = track_draft.clone();
        track_editor_window.on_update_track_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(track_window) = weak_track_window.upgrade() else {
                return;
            };
            if let Some(draft) = track_draft.borrow_mut().as_mut() {
                draft.name = value.to_string();
                window.set_track_draft_name(value.clone());
                track_window.set_track_name(value);
            }
        });
    }

    {
        let weak_window = window.as_weak();
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let input_track_devices = input_track_devices.clone();
        let track_input_channels = track_input_channels.clone();
        window.on_select_track_input_device(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = track_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let Some(device) = input_track_devices.get(index as usize) else {
                return;
            };
            draft.input_device_id = Some(device.id.clone());
            draft.input_channels.clear();
            if let Some(session) = project_session.borrow().as_ref() {
                replace_channel_options(
                    &track_input_channels,
                    build_input_channel_items(draft, &session.project, &input_track_devices),
                );
            }
            window.set_selected_track_input_device_index(selected_device_index(
                &input_track_devices,
                draft.input_device_id.as_deref(),
            ));
        });
    }

    {
        let weak_window = window.as_weak();
        let weak_track_window = track_editor_window.as_weak();
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let input_track_devices = input_track_devices.clone();
        let track_input_channels = track_input_channels.clone();
        track_editor_window.on_select_track_input_device(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(track_window) = weak_track_window.upgrade() else {
                return;
            };
            let mut draft_borrow = track_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let Some(device) = input_track_devices.get(index as usize) else {
                return;
            };
            draft.input_device_id = Some(device.id.clone());
            draft.input_channels.clear();
            if let Some(session) = project_session.borrow().as_ref() {
                replace_channel_options(
                    &track_input_channels,
                    build_input_channel_items(draft, &session.project, &input_track_devices),
                );
            }
            let selected_index = selected_device_index(&input_track_devices, draft.input_device_id.as_deref());
            window.set_selected_track_input_device_index(selected_index);
            track_window.set_selected_input_device_index(selected_index);
        });
    }

    {
        let weak_window = window.as_weak();
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let output_track_devices = output_track_devices.clone();
        let track_output_channels = track_output_channels.clone();
        window.on_select_track_output_device(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = track_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let Some(device) = output_track_devices.get(index as usize) else {
                return;
            };
            draft.output_device_id = Some(device.id.clone());
            draft.output_channels.clear();
            if project_session.borrow().as_ref().is_some() {
                replace_channel_options(
                    &track_output_channels,
                    build_output_channel_items(draft, &output_track_devices),
                );
            }
            window.set_selected_track_output_device_index(selected_device_index(
                &output_track_devices,
                draft.output_device_id.as_deref(),
            ));
        });
    }

    {
        let weak_window = window.as_weak();
        let weak_track_window = track_editor_window.as_weak();
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let output_track_devices = output_track_devices.clone();
        let track_output_channels = track_output_channels.clone();
        track_editor_window.on_select_track_output_device(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(track_window) = weak_track_window.upgrade() else {
                return;
            };
            let mut draft_borrow = track_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let Some(device) = output_track_devices.get(index as usize) else {
                return;
            };
            draft.output_device_id = Some(device.id.clone());
            draft.output_channels.clear();
            if project_session.borrow().as_ref().is_some() {
                replace_channel_options(
                    &track_output_channels,
                    build_output_channel_items(draft, &output_track_devices),
                );
            }
            let selected_index = selected_device_index(&output_track_devices, draft.output_device_id.as_deref());
            window.set_selected_track_output_device_index(selected_index);
            track_window.set_selected_output_device_index(selected_index);
        });
    }

    {
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let input_track_devices = input_track_devices.clone();
        let track_input_channels = track_input_channels.clone();
        let weak_window = window.as_weak();
        window.on_toggle_track_input_channel(move |index, selected| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = track_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let channel = index as usize;
            let Some(option) = track_input_channels.row_data(index as usize) else {
                return;
            };
            if selected && !option.available && !option.selected {
                window.set_status_message("Canal de entrada já está em uso por outra track.".into());
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
                    &track_input_channels,
                    build_input_channel_items(draft, &session.project, &input_track_devices),
                );
            }
        });
    }

    {
        let weak_track_window = track_editor_window.as_weak();
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let input_track_devices = input_track_devices.clone();
        let track_input_channels = track_input_channels.clone();
        track_editor_window.on_toggle_track_input_channel(move |index, selected| {
            let Some(track_window) = weak_track_window.upgrade() else {
                return;
            };
            let mut draft_borrow = track_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let channel = index as usize;
            let Some(option) = track_input_channels.row_data(index as usize) else {
                return;
            };
            if selected && !option.available && !option.selected {
                track_window.set_status_message("Canal de entrada já está em uso por outra track.".into());
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
                    &track_input_channels,
                    build_input_channel_items(draft, &session.project, &input_track_devices),
                );
            }
            track_window.set_status_message("".into());
        });
    }

    {
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let output_track_devices = output_track_devices.clone();
        let track_output_channels = track_output_channels.clone();
        window.on_toggle_track_output_channel(move |index, selected| {
            let mut draft_borrow = track_draft.borrow_mut();
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
                    &track_output_channels,
                    build_output_channel_items(draft, &output_track_devices),
                );
            }
        });
    }

    {
        let weak_track_window = track_editor_window.as_weak();
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let output_track_devices = output_track_devices.clone();
        let track_output_channels = track_output_channels.clone();
        track_editor_window.on_toggle_track_output_channel(move |index, selected| {
            let Some(track_window) = weak_track_window.upgrade() else {
                return;
            };
            let mut draft_borrow = track_draft.borrow_mut();
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
                    &track_output_channels,
                    build_output_channel_items(draft, &output_track_devices),
                );
            }
            track_window.set_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let selected_stage = selected_stage.clone();
        let stage_editor_draft = stage_editor_draft.clone();
        let stage_model_options = stage_model_options.clone();
        let stage_parameter_items = stage_parameter_items.clone();
        let project_session = project_session.clone();
        window.on_select_track_stage(move |track_index, stage_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let Some(track) = session.project.tracks.get(track_index as usize) else {
                window.set_status_message("Track inválida.".into());
                return;
            };
            let Some(block) = track.blocks.get(stage_index as usize) else {
                window.set_status_message("Stage inválido.".into());
                return;
            };
            let Some((effect_type, model_id, params, enabled)) = block_editor_data(block) else {
                window.set_status_message("Esse stage ainda não pode ser editado pela GUI.".into());
                return;
            };

            *selected_stage.borrow_mut() = Some(SelectedStage {
                track_index: track_index as usize,
                stage_index: stage_index as usize,
            });
            *stage_editor_draft.borrow_mut() = Some(StageEditorDraft {
                track_index: track_index as usize,
                stage_index: Some(stage_index as usize),
                before_index: stage_index as usize,
                effect_type: effect_type.clone(),
                model_id: model_id.clone(),
                enabled,
            });
            stage_model_options.set_vec(stage_model_picker_items(&effect_type));
            stage_parameter_items.set_vec(stage_parameter_items_for_model(&effect_type, &model_id, &params));
            set_selected_stage(&window, selected_stage.borrow().as_ref());
            let drawer_state = stage_drawer_state(Some(stage_index as usize), &effect_type, Some(&model_id));
            window.set_stage_drawer_title(drawer_state.title.into());
            window.set_stage_drawer_confirm_label(drawer_state.confirm_label.into());
            window.set_stage_drawer_edit_mode(true);
            window.set_stage_drawer_selected_type_index(stage_type_index(&effect_type));
            window.set_stage_drawer_selected_model_index(stage_model_index(&effect_type, &model_id));
            window.set_stage_drawer_enabled(enabled);
            window.set_stage_drawer_status_message("".into());
            window.set_show_stage_type_picker(false);
            window.set_show_stage_drawer(true);
        });
    }

    {
        let weak_window = window.as_weak();
        let selected_stage = selected_stage.clone();
        let stage_editor_draft = stage_editor_draft.clone();
        let stage_model_options = stage_model_options.clone();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_clear_track_stage(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            *selected_stage.borrow_mut() = None;
            *stage_editor_draft.borrow_mut() = None;
            stage_model_options.set_vec(Vec::new());
            stage_parameter_items.set_vec(Vec::new());
            set_selected_stage(&window, None);
            window.set_show_stage_drawer(false);
            window.set_show_stage_type_picker(false);
            window.set_stage_drawer_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let selected_stage = selected_stage.clone();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_toggle_track_stage_enabled(move |track_index, stage_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let Some(track) = session.project.tracks.get_mut(track_index as usize) else {
                window.set_status_message("Track inválida.".into());
                return;
            };
            let Some(block) = track.blocks.get_mut(stage_index as usize) else {
                window.set_status_message("Stage inválido.".into());
                return;
            };
            block.enabled = !block.enabled;
            let was_running = project_streams.borrow().is_some();
            if was_running {
                stop_project_runtime(&project_streams);
                window.set_project_running(false);
            }
            replace_project_tracks(&project_tracks, &session.project);
            *selected_stage.borrow_mut() = Some(SelectedStage {
                track_index: track_index as usize,
                stage_index: stage_index as usize,
            });
            set_selected_stage(&window, selected_stage.borrow().as_ref());
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            window.set_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let selected_stage = selected_stage.clone();
        let stage_editor_draft = stage_editor_draft.clone();
        let stage_model_options = stage_model_options.clone();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_start_stage_insert(move |track_index, before_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            *selected_stage.borrow_mut() = None;
            *stage_editor_draft.borrow_mut() = Some(StageEditorDraft {
                track_index: track_index as usize,
                stage_index: None,
                before_index: before_index as usize,
                effect_type: String::new(),
                model_id: String::new(),
                enabled: true,
            });
            stage_model_options.set_vec(Vec::new());
            stage_parameter_items.set_vec(Vec::new());
            set_selected_stage(&window, None);
            window.set_stage_drawer_edit_mode(false);
            window.set_stage_drawer_selected_type_index(-1);
            window.set_stage_drawer_selected_model_index(-1);
            window.set_stage_drawer_status_message("".into());
            window.set_show_stage_drawer(false);
            window.set_show_stage_type_picker(true);
        });
    }

    {
        let weak_window = window.as_weak();
        let stage_editor_draft = stage_editor_draft.clone();
        let stage_model_options = stage_model_options.clone();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_choose_stage_type(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(stage_type) = stage_types().get(index as usize).cloned() else {
                return;
            };
            let models = stage_models_for_type(stage_type.effect_type);
            let Some(model) = models.first() else {
                return;
            };
            if let Some(draft) = stage_editor_draft.borrow_mut().as_mut() {
                draft.effect_type = stage_type.effect_type.to_string();
                draft.model_id = model.model_id.to_string();
            }
            stage_model_options.set_vec(stage_model_picker_items(stage_type.effect_type));
            stage_parameter_items.set_vec(stage_parameter_items_for_model(
                stage_type.effect_type,
                model.model_id,
                &ParameterSet::default(),
            ));
            let drawer_state = stage_drawer_state(None, stage_type.effect_type, Some(model.model_id));
            window.set_stage_drawer_title(drawer_state.title.into());
            window.set_stage_drawer_confirm_label(drawer_state.confirm_label.into());
            window.set_stage_drawer_edit_mode(false);
            window.set_stage_drawer_selected_type_index(index);
            window.set_stage_drawer_selected_model_index(0);
            window.set_stage_drawer_status_message("".into());
            window.set_show_stage_type_picker(false);
            window.set_show_stage_drawer(true);
        });
    }

    {
        let weak_window = window.as_weak();
        let stage_editor_draft = stage_editor_draft.clone();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_choose_stage_model(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = stage_editor_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let models = stage_models_for_type(&draft.effect_type);
            let Some(model) = models.get(index as usize) else {
                return;
            };
            draft.model_id = model.model_id.to_string();
            stage_parameter_items.set_vec(stage_parameter_items_for_model(
                &draft.effect_type,
                model.model_id,
                &ParameterSet::default(),
            ));
            window.set_stage_drawer_selected_model_index(index);
            window.set_stage_drawer_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let stage_editor_draft = stage_editor_draft.clone();
        let stage_model_options = stage_model_options.clone();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_cancel_stage_picker(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            *stage_editor_draft.borrow_mut() = None;
            stage_model_options.set_vec(Vec::new());
            stage_parameter_items.set_vec(Vec::new());
            window.set_stage_drawer_selected_model_index(-1);
            window.set_stage_drawer_selected_type_index(-1);
            window.set_show_stage_type_picker(false);
            window.set_show_stage_drawer(false);
            window.set_stage_drawer_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let selected_stage = selected_stage.clone();
        let stage_editor_draft = stage_editor_draft.clone();
        let stage_model_options = stage_model_options.clone();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_close_stage_drawer(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            *selected_stage.borrow_mut() = None;
            *stage_editor_draft.borrow_mut() = None;
            stage_model_options.set_vec(Vec::new());
            stage_parameter_items.set_vec(Vec::new());
            window.set_stage_drawer_selected_model_index(-1);
            window.set_stage_drawer_selected_type_index(-1);
            set_selected_stage(&window, None);
            window.set_show_stage_type_picker(false);
            window.set_show_stage_drawer(false);
            window.set_stage_drawer_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let stage_editor_draft = stage_editor_draft.clone();
        window.on_toggle_stage_drawer_enabled(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = stage_editor_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            draft.enabled = !draft.enabled;
            window.set_stage_drawer_enabled(draft.enabled);
        });
    }

    {
        let weak_window = window.as_weak();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_update_stage_parameter_text(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_stage_parameter_text(&stage_parameter_items, path.as_str(), value.as_str());
            window.set_stage_drawer_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_update_stage_parameter_number(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_stage_parameter_number(&stage_parameter_items, path.as_str(), value);
            window.set_stage_drawer_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_update_stage_parameter_bool(move |path, value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_stage_parameter_bool(&stage_parameter_items, path.as_str(), value);
            window.set_stage_drawer_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_select_stage_parameter_option(move |path, index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            set_stage_parameter_option(&stage_parameter_items, path.as_str(), index);
            window.set_stage_drawer_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let stage_parameter_items = stage_parameter_items.clone();
        window.on_pick_stage_parameter_file(move |path| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let extensions = stage_parameter_extensions(&stage_parameter_items, path.as_str());
            let mut dialog = FileDialog::new();
            if !extensions.is_empty() {
                let refs = extensions.iter().map(|value| value.as_str()).collect::<Vec<_>>();
                dialog = dialog.add_filter("Arquivos suportados", &refs);
            }
            let Some(file) = dialog.pick_file() else {
                return;
            };
            set_stage_parameter_text(
                &stage_parameter_items,
                path.as_str(),
                file.to_string_lossy().as_ref(),
            );
            window.set_stage_drawer_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let selected_stage = selected_stage.clone();
        let stage_editor_draft = stage_editor_draft.clone();
        let stage_model_options = stage_model_options.clone();
        let stage_parameter_items = stage_parameter_items.clone();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_save_stage_drawer(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(draft) = stage_editor_draft.borrow().clone() else {
                return;
            };

            let params = match stage_parameter_values(
                &stage_parameter_items,
                &draft.effect_type,
                &draft.model_id,
            ) {
                Ok(params) => params,
                Err(error) => {
                    window.set_stage_drawer_status_message(error.to_string().into());
                    return;
                }
            };

            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                window.set_stage_drawer_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let Some(track) = session.project.tracks.get_mut(draft.track_index) else {
                window.set_stage_drawer_status_message("Track inválida.".into());
                return;
            };
            let kind = match build_block_kind(&draft.effect_type, &draft.model_id, params) {
                Ok(kind) => kind,
                Err(error) => {
                    window.set_stage_drawer_status_message(error.to_string().into());
                    return;
                }
            };

            if let Some(stage_index) = draft.stage_index {
                let Some(block) = track.blocks.get_mut(stage_index) else {
                    window.set_stage_drawer_status_message("Stage inválido.".into());
                    return;
                };
                let block_id = block.id.clone();
                block.enabled = draft.enabled;
                block.kind = kind;
                block.id = block_id;
            } else {
                let insert_index = draft.before_index.min(track.blocks.len());
                track.blocks.insert(
                    insert_index,
                    AudioBlock {
                        id: BlockId(format!("{}:block:new", track.id.0)),
                        enabled: draft.enabled,
                        kind,
                    },
                );
                reassign_track_block_ids(track);
            }

            if project_streams.borrow().is_some() {
                stop_project_runtime(&project_streams);
                window.set_project_running(false);
            }

            replace_project_tracks(&project_tracks, &session.project);
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            *selected_stage.borrow_mut() = None;
            set_selected_stage(&window, None);
            *stage_editor_draft.borrow_mut() = None;
            stage_model_options.set_vec(Vec::new());
            stage_parameter_items.set_vec(Vec::new());
            window.set_stage_drawer_selected_model_index(-1);
            window.set_stage_drawer_selected_type_index(-1);
            window.set_show_stage_drawer(false);
            window.set_stage_drawer_status_message("".into());
            window.set_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let selected_stage = selected_stage.clone();
        let stage_editor_draft = stage_editor_draft.clone();
        let stage_model_options = stage_model_options.clone();
        let stage_parameter_items = stage_parameter_items.clone();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_delete_stage_drawer(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(draft) = stage_editor_draft.borrow().clone() else {
                return;
            };
            let Some(stage_index) = draft.stage_index else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                window.set_stage_drawer_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let Some(track) = session.project.tracks.get_mut(draft.track_index) else {
                window.set_stage_drawer_status_message("Track inválida.".into());
                return;
            };
            if stage_index >= track.blocks.len() {
                window.set_stage_drawer_status_message("Stage inválido.".into());
                return;
            }
            track.blocks.remove(stage_index);
            reassign_track_block_ids(track);
            if project_streams.borrow().is_some() {
                stop_project_runtime(&project_streams);
                window.set_project_running(false);
            }
            replace_project_tracks(&project_tracks, &session.project);
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            *selected_stage.borrow_mut() = None;
            *stage_editor_draft.borrow_mut() = None;
            stage_model_options.set_vec(Vec::new());
            stage_parameter_items.set_vec(Vec::new());
            set_selected_stage(&window, None);
            window.set_show_stage_drawer(false);
            window.set_stage_drawer_status_message("".into());
            window.set_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_save_track(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let draft = match track_draft.borrow().clone() {
                Some(draft) => draft,
                None => {
                    window.set_status_message("Nenhuma track em edição.".into());
                    return;
                }
            };
            if draft.input_device_id.is_none() {
                window.set_status_message("Selecione o dispositivo de entrada.".into());
                return;
            }
            if draft.output_device_id.is_none() {
                window.set_status_message("Selecione o dispositivo de saída.".into());
                return;
            }
            if draft.input_channels.is_empty() {
                window.set_status_message("Selecione pelo menos um canal de entrada.".into());
                return;
            }
            if draft.output_channels.is_empty() {
                window.set_status_message("Selecione pelo menos um canal de saída.".into());
                return;
            }

            let editing_index = draft.editing_index;
            let existing_track = editing_index.and_then(|index| session.project.tracks.get(index).cloned());
            let track = Track {
                id: existing_track
                    .as_ref()
                    .map(|track| track.id.clone())
                    .unwrap_or_else(|| TrackId(format!("track:{}", session.project.tracks.len()))),
                description: normalized_track_description(&draft.name),
                enabled: existing_track.as_ref().map(|track| track.enabled).unwrap_or(true),
                input_device_id: DeviceId(draft.input_device_id.unwrap_or_default()),
                input_channels: draft.input_channels,
                output_device_id: DeviceId(draft.output_device_id.unwrap_or_default()),
                output_channels: draft.output_channels,
                blocks: existing_track
                    .as_ref()
                    .map(|track| track.blocks.clone())
                    .unwrap_or_default(),
                output_mixdown: existing_track
                    .as_ref()
                    .map(|track| track.output_mixdown)
                    .unwrap_or(TrackOutputMixdown::Average),
            };
            if let Some(index) = editing_index {
                if let Some(current) = session.project.tracks.get_mut(index) {
                    *current = track;
                }
            } else {
                session.project.tracks.push(track);
            }
            let was_running = project_streams.borrow().is_some();
            if was_running {
                stop_project_runtime(&project_streams);
                window.set_project_running(false);
            }
            replace_project_tracks(&project_tracks, &session.project);
            *track_draft.borrow_mut() = None;
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            window.set_status_message("".into());
            window.set_show_track_editor(false);
        });
    }

    {
        let weak_window = window.as_weak();
        let weak_track_window = track_editor_window.as_weak();
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        track_editor_window.on_save_track(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(track_window) = weak_track_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                track_window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let draft = match track_draft.borrow().clone() {
                Some(draft) => draft,
                None => {
                    track_window.set_status_message("Nenhuma track em edição.".into());
                    return;
                }
            };
            if draft.input_device_id.is_none() {
                track_window.set_status_message("Selecione o dispositivo de entrada.".into());
                return;
            }
            if draft.output_device_id.is_none() {
                track_window.set_status_message("Selecione o dispositivo de saída.".into());
                return;
            }
            if draft.input_channels.is_empty() {
                track_window.set_status_message("Selecione pelo menos um canal de entrada.".into());
                return;
            }
            if draft.output_channels.is_empty() {
                track_window.set_status_message("Selecione pelo menos um canal de saída.".into());
                return;
            }

            let editing_index = draft.editing_index;
            let existing_track = editing_index.and_then(|index| session.project.tracks.get(index).cloned());
            let track = Track {
                id: existing_track
                    .as_ref()
                    .map(|track| track.id.clone())
                    .unwrap_or_else(|| TrackId(format!("track:{}", session.project.tracks.len()))),
                description: normalized_track_description(&draft.name),
                enabled: existing_track.as_ref().map(|track| track.enabled).unwrap_or(true),
                input_device_id: DeviceId(draft.input_device_id.unwrap_or_default()),
                input_channels: draft.input_channels,
                output_device_id: DeviceId(draft.output_device_id.unwrap_or_default()),
                output_channels: draft.output_channels,
                blocks: existing_track
                    .as_ref()
                    .map(|track| track.blocks.clone())
                    .unwrap_or_default(),
                output_mixdown: existing_track
                    .as_ref()
                    .map(|track| track.output_mixdown)
                    .unwrap_or(TrackOutputMixdown::Average),
            };
            if let Some(index) = editing_index {
                if let Some(current) = session.project.tracks.get_mut(index) {
                    *current = track;
                }
            } else {
                session.project.tracks.push(track);
            }
            let was_running = project_streams.borrow().is_some();
            if was_running {
                stop_project_runtime(&project_streams);
                window.set_project_running(false);
            }
            replace_project_tracks(&project_tracks, &session.project);
            *track_draft.borrow_mut() = None;
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            track_window.set_status_message("".into());
            window.set_status_message("".into());
            window.set_show_track_editor(false);
            let _ = track_window.hide();
        });
    }

    {
        let weak_window = window.as_weak();
        let track_draft = track_draft.clone();
        window.on_cancel_track(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            *track_draft.borrow_mut() = None;
            window.set_status_message("".into());
            window.set_show_track_editor(false);
        });
    }

    {
        let weak_window = window.as_weak();
        let weak_track_window = track_editor_window.as_weak();
        let track_draft = track_draft.clone();
        track_editor_window.on_cancel_track(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(track_window) = weak_track_window.upgrade() else {
                return;
            };
            *track_draft.borrow_mut() = None;
            track_window.set_status_message("".into());
            window.set_status_message("".into());
            window.set_show_track_editor(false);
            let _ = track_window.hide();
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_remove_track(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let index = index as usize;
            if index >= session.project.tracks.len() {
                window.set_status_message("Track inválida.".into());
                return;
            }

            session.project.tracks.remove(index);
            let was_running = project_streams.borrow().is_some();
            if was_running {
                stop_project_runtime(&project_streams);
                window.set_project_running(false);
            }
            replace_project_tracks(&project_tracks, &session.project);
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            window.set_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        window.on_toggle_track_enabled(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let Some(track) = session.project.tracks.get_mut(index as usize) else {
                window.set_status_message("Track inválida.".into());
                return;
            };
            track.enabled = !track.enabled;
            replace_project_tracks(&project_tracks, &session.project);
            sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty);
            window.set_status_message("".into());
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_streams = project_streams.clone();
        window.on_start_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            match start_project_runtime(session) {
                Ok(streams) => {
                    *project_streams.borrow_mut() = Some(streams);
                    window.set_project_running(true);
                    window.set_status_message("".into());
                }
                Err(error) => {
                    window.set_project_running(false);
                    window.set_status_message(error.to_string().into());
                }
            }
        });
    }

    {
        let weak_window = window.as_weak();
        let project_streams = project_streams.clone();
        window.on_stop_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            stop_project_runtime(&project_streams);
            window.set_project_running(false);
            window.set_status_message("".into());
        });
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

fn recent_project_items(recent_projects: &[RecentProjectEntry], query: &str) -> Vec<RecentProjectItem> {
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

fn create_new_project_session(default_config_path: &PathBuf) -> ProjectSession {
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
        tracks: Vec::new(),
    };
    ProjectSession {
        project,
        project_path: None,
        config_path: None,
        presets_path: config.presets_path.unwrap_or_else(|| PathBuf::from("./presets")),
    }
}

fn load_app_config(path: &PathBuf) -> Result<AppConfigYaml> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
}

fn resolve_project_config_path(project_path: &PathBuf) -> PathBuf {
    project_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("config.yaml")
}

fn load_project_session(project_path: &PathBuf, config_path: &PathBuf) -> Result<ProjectSession> {
    let config = if config_path.exists() {
        load_app_config(config_path)?
    } else {
        AppConfigYaml::default()
    };
    let presets_path = config
        .presets_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("./presets"));
    let project = YamlProjectRepository { path: project_path.clone() }.load_current_project()?;
    Ok(ProjectSession {
        project,
        project_path: Some(project_path.clone()),
        config_path: Some(config_path.clone()),
        presets_path: project_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(presets_path),
    })
}

fn replace_project_tracks(model: &Rc<VecModel<ProjectTrackItem>>, project: &Project) {
    let items = project
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| ProjectTrackItem {
            title: track
                .description
                .clone()
                .unwrap_or_else(|| format!("Track {}", index + 1))
                .into(),
            subtitle: track_routing_summary(track).into(),
            enabled: track.enabled,
            stage_count_label: if track.blocks.len() == 1 {
                "1 stage".into()
            } else {
                format!("{} stages", track.blocks.len()).into()
            },
            stages: ModelRc::from(Rc::new(VecModel::from(
                track.blocks.iter().map(track_stage_item_from_block).collect::<Vec<_>>(),
            ))),
        })
        .collect::<Vec<_>>();
    model.set_vec(items);
}

fn stage_type_picker_items() -> Vec<StageTypePickerItem> {
    stage_types()
        .into_iter()
        .map(|item| StageTypePickerItem {
            effect_type: item.effect_type.into(),
            label: item.label.into(),
            subtitle: format!("Escolher um {} para a cadeia", item.label).into(),
            icon_kind: item.icon_kind.into(),
        })
        .collect()
}

fn stage_model_picker_items(effect_type: &str) -> Vec<StageModelPickerItem> {
    stage_models_for_type(effect_type)
        .into_iter()
        .map(|item| StageModelPickerItem {
            effect_type: item.effect_type.into(),
            model_id: item.model_id.into(),
            label: item.title.into(),
            subtitle: item.subtitle.into(),
            icon_kind: item.icon_kind.into(),
        })
        .collect()
}

fn set_selected_stage(window: &AppWindow, selected_stage: Option<&SelectedStage>) {
    if let Some(selected_stage) = selected_stage {
        window.set_selected_track_stage_track_index(selected_stage.track_index as i32);
        window.set_selected_track_stage_index(selected_stage.stage_index as i32);
    } else {
        window.set_selected_track_stage_track_index(-1);
        window.set_selected_track_stage_index(-1);
    }
}

fn stage_type_index(effect_type: &str) -> i32 {
    stage_types()
        .iter()
        .position(|item| item.effect_type == effect_type)
        .map(|index| index as i32)
        .unwrap_or(-1)
}

fn stage_model_index(effect_type: &str, model_id: &str) -> i32 {
    stage_models_for_type(effect_type)
        .iter()
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

fn block_editor_data(block: &AudioBlock) -> Option<(String, String, ParameterSet, bool)> {
    match &block.kind {
        AudioBlockKind::Nam(stage) => Some((
            "nam".to_string(),
            stage.model.clone(),
            stage.params.clone(),
            block.enabled,
        )),
        AudioBlockKind::Core(core) => match &core.kind {
            CoreBlockKind::AmpHead(stage) => Some(("amp_head".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::AmpCombo(stage) => Some(("amp_combo".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::Cab(stage) => Some(("cab".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::FullRig(stage) => Some(("full_rig".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::Drive(stage) => Some(("drive".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::Compressor(stage) => Some(("compressor".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::Gate(stage) => Some(("gate".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::Eq(stage) => Some(("eq".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::Tremolo(stage) => Some(("tremolo".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::Delay(stage) => Some(("delay".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::Reverb(stage) => Some(("reverb".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
            CoreBlockKind::Tuner(stage) => Some(("tuner".to_string(), stage.model.clone(), stage.params.clone(), block.enabled)),
        },
        _ => None,
    }
}

fn stage_parameter_items_for_model(
    effect_type: &str,
    model_id: &str,
    params: &ParameterSet,
) -> Vec<StageParameterItem> {
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return Vec::new();
    };
    let Ok(normalized) = params.normalized_against(&schema) else {
        return Vec::new();
    };
    schema
        .parameters
        .iter()
        .filter(|spec| stage_parameter_visible_in_gui(effect_type, &spec.path))
        .map(|spec| {
            let current = normalized
                .get(&spec.path)
                .cloned()
                .or_else(|| spec.default_value.clone())
                .unwrap_or(domain::value_objects::ParameterValue::Null);
            let (numeric_value, numeric_min, numeric_max) = match &spec.domain {
                ParameterDomain::IntRange { min, max, .. } => (
                    current.as_i64().unwrap_or(*min) as f32,
                    *min as f32,
                    *max as f32,
                ),
                ParameterDomain::FloatRange { min, max, .. } => (
                    current.as_f32().unwrap_or(*min),
                    *min,
                    *max,
                ),
                _ => (0.0, 0.0, 1.0),
            };
            let (option_labels, option_values, selected_option_index, file_extensions) =
                match &spec.domain {
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
            StageParameterItem {
                path: spec.path.clone().into(),
                label: spec.label.clone().into(),
                group: spec.group.clone().unwrap_or_default().into(),
                widget_kind: match &spec.domain {
                    ParameterDomain::Bool => "bool",
                    ParameterDomain::IntRange { .. } => "int",
                    ParameterDomain::FloatRange { .. } => "float",
                    ParameterDomain::Enum { .. } => "enum",
                    ParameterDomain::Text => "text",
                    ParameterDomain::FilePath { .. } => "path",
                }
                .into(),
                unit_text: unit_label(&spec.unit).into(),
                value_text: match current {
                    domain::value_objects::ParameterValue::String(ref value) => value.clone().into(),
                    domain::value_objects::ParameterValue::Int(value) => value.to_string().into(),
                    domain::value_objects::ParameterValue::Float(value) => format!("{value:.2}").into(),
                    domain::value_objects::ParameterValue::Bool(value) => {
                        if value { "true".into() } else { "false".into() }
                    }
                    domain::value_objects::ParameterValue::Null => "".into(),
                },
                numeric_value,
                numeric_min,
                numeric_max,
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

fn stage_parameter_visible_in_gui(effect_type: &str, path: &str) -> bool {
    if path == "enabled" {
        return false;
    }

    match effect_type {
        "amp_head" => !matches!(
            path,
            "input_level_db"
                | "output_level_db"
                | "noise_gate_threshold_db"
                | "noise_gate_enabled"
                | "eq_enabled"
        ),
        _ => true,
    }
}

fn set_stage_parameter_text(
    model: &Rc<VecModel<StageParameterItem>>,
    path: &str,
    value: &str,
) {
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

fn set_stage_parameter_bool(
    model: &Rc<VecModel<StageParameterItem>>,
    path: &str,
    value: bool,
) {
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

fn set_stage_parameter_number(
    model: &Rc<VecModel<StageParameterItem>>,
    path: &str,
    value: f32,
) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                row.numeric_value = value;
                row.value_text = if row.widget_kind.as_str() == "int" {
                    format!("{:.0}", value.round()).into()
                } else {
                    format!("{value:.2}").into()
                };
                model.set_row_data(index, row);
                break;
            }
        }
    }
}

fn set_stage_parameter_option(
    model: &Rc<VecModel<StageParameterItem>>,
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

fn stage_parameter_extensions(
    model: &Rc<VecModel<StageParameterItem>>,
    path: &str,
) -> Vec<String> {
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

fn stage_parameter_values(
    model: &Rc<VecModel<StageParameterItem>>,
    effect_type: &str,
    model_id: &str,
) -> Result<ParameterSet> {
    let schema = schema_for_block_model(effect_type, model_id).map_err(|error| anyhow!(error))?;
    let mut params = ParameterSet::default();
    for index in 0..model.row_count() {
        let Some(row) = model.row_data(index) else {
            continue;
        };
        let value = match row.widget_kind.as_str() {
            "bool" => domain::value_objects::ParameterValue::Bool(row.bool_value),
            "int" => {
                domain::value_objects::ParameterValue::Int(row.numeric_value.round() as i64)
            }
            "float" => {
                domain::value_objects::ParameterValue::Float(row.numeric_value)
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

fn build_block_kind(effect_type: &str, model_id: &str, params: ParameterSet) -> Result<AudioBlockKind> {
    let model = model_id.to_string();
    let kind = match effect_type {
        "amp_head" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::AmpHead(AmpHeadBlock { model, params }),
        }),
        "amp_combo" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::AmpCombo(AmpComboBlock { model, params }),
        }),
        "cab" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Cab(CabBlock { model, params }),
        }),
        "full_rig" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::FullRig(FullRigBlock { model, params }),
        }),
        "drive" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Drive(DriveBlock { model, params }),
        }),
        "compressor" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Compressor(CompressorBlock { model, params }),
        }),
        "gate" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Gate(GateBlock { model, params }),
        }),
        "eq" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Eq(EqBlock { model, params }),
        }),
        "tremolo" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Tremolo(TremoloBlock { model, params }),
        }),
        "delay" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Delay(DelayBlock { model, params }),
        }),
        "reverb" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Reverb(ReverbBlock { model, params }),
        }),
        "tuner" => AudioBlockKind::Core(CoreBlock {
            kind: CoreBlockKind::Tuner(TunerBlock { model, params }),
        }),
        "nam" => AudioBlockKind::Nam(NamBlock { model, params }),
        other => return Err(anyhow!("tipo de stage '{}' não suportado na GUI", other)),
    };
    Ok(kind)
}

fn reassign_track_block_ids(track: &mut Track) {
    for (index, block) in track.blocks.iter_mut().enumerate() {
        block.id = BlockId(format!("{}:block:{}", track.id.0, index));
    }
}

fn set_device_selection_from_project(
    model: &Rc<VecModel<DeviceSelectionItem>>,
    device_settings: &[DeviceSettings],
) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if let Some(setting) = device_settings
                .iter()
                .find(|setting| row.device_id == setting.device_id.0)
            {
                row.selected = true;
                row.sample_rate_text = setting.sample_rate.to_string().into();
                row.buffer_size_text = setting.buffer_size_frames.to_string().into();
            } else {
                row.selected = false;
            }
            model.set_row_data(index, row);
        }
    }
}

fn merge_device_settings(
    input_devices: Vec<GuiAudioDeviceSettings>,
    output_devices: Vec<GuiAudioDeviceSettings>,
) -> Vec<DeviceSettings> {
    let mut merged: Vec<DeviceSettings> = Vec::new();
    for device in input_devices.into_iter().chain(output_devices) {
        if merged
            .iter()
            .any(|current| current.device_id.0 == device.device_id)
        {
            continue;
        }
        merged.push(DeviceSettings {
            device_id: DeviceId(device.device_id),
            sample_rate: device.sample_rate,
            buffer_size_frames: device.buffer_size_frames,
        });
    }
    merged
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
        tracks: session
            .project
            .tracks
            .iter()
            .map(|track| -> Result<ProjectTrackYaml> {
                Ok(ProjectTrackYaml {
                    description: track.description.clone(),
                    enabled: track.enabled,
                    input_device_id: track.input_device_id.0.clone(),
                    input_channels: track.input_channels.clone(),
                    output_device_id: track.output_device_id.0.clone(),
                    output_channels: track.output_channels.clone(),
                    stages: serialize_audio_blocks(&track.blocks)?,
                    output_mixdown: track.output_mixdown,
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
    fs::create_dir_all(&parent_dir.join("presets"))?;
    Ok(())
}

fn save_track_blocks_to_preset(track: &Track, path: &PathBuf) -> Result<()> {
    let preset = TrackBlocksPreset {
        id: preset_id_from_path(path)?,
        name: track.description.clone(),
        blocks: track.blocks.clone(),
    };
    save_track_preset_file(path, &preset)
}

fn load_preset_file(path: &PathBuf) -> Result<TrackBlocksPreset> {
    load_track_preset_file(path)
}

fn preset_id_from_path(path: &PathBuf) -> Result<String> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow!("arquivo de preset inválido"))
}

fn project_title_for_path(project_path: Option<&PathBuf>, project: &Project) -> String {
    if let Some(name) = project.name.as_ref().map(|name| name.trim()).filter(|name| !name.is_empty()) {
        return name.to_string();
    }
    project_path
        .and_then(|path| path.file_stem())
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| {
            if project.tracks.is_empty() {
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

fn create_track_draft(
    project: &Project,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) -> TrackDraft {
    TrackDraft {
        editing_index: None,
        name: format!("Track {}", project.tracks.len() + 1),
        input_device_id: input_devices.first().map(|device| device.id.clone()),
        output_device_id: output_devices.first().map(|device| device.id.clone()),
        input_channels: Vec::new(),
        output_channels: Vec::new(),
    }
}

fn track_draft_from_track(index: usize, track: &Track) -> TrackDraft {
    TrackDraft {
        editing_index: Some(index),
        name: track
            .description
            .clone()
            .unwrap_or_else(|| format!("Track {}", index + 1)),
        input_device_id: Some(track.input_device_id.0.clone()),
        output_device_id: Some(track.output_device_id.0.clone()),
        input_channels: track.input_channels.clone(),
        output_channels: track.output_channels.clone(),
    }
}

fn track_stage_item_from_block(block: &AudioBlock) -> TrackStageItem {
    let (kind, label) = match &block.kind {
        AudioBlockKind::Nam(stage) => ("nam".to_string(), stage.model.clone()),
        AudioBlockKind::Core(core) => match &core.kind {
            CoreBlockKind::AmpHead(stage) => ("amp_head".to_string(), stage.model.clone()),
            CoreBlockKind::AmpCombo(stage) => ("amp_combo".to_string(), stage.model.clone()),
            CoreBlockKind::Cab(stage) => ("cab".to_string(), stage.model.clone()),
            CoreBlockKind::FullRig(stage) => ("full_rig".to_string(), stage.model.clone()),
            CoreBlockKind::Drive(stage) => ("drive".to_string(), stage.model.clone()),
            CoreBlockKind::Compressor(stage) => ("compressor".to_string(), stage.model.clone()),
            CoreBlockKind::Gate(stage) => ("gate".to_string(), stage.model.clone()),
            CoreBlockKind::Eq(stage) => ("eq".to_string(), stage.model.clone()),
            CoreBlockKind::Tremolo(stage) => ("tremolo".to_string(), stage.model.clone()),
            CoreBlockKind::Delay(stage) => ("delay".to_string(), stage.model.clone()),
            CoreBlockKind::Reverb(stage) => ("reverb".to_string(), stage.model.clone()),
            CoreBlockKind::Tuner(stage) => ("tuner".to_string(), stage.model.clone()),
        },
        AudioBlockKind::Select(_) => ("core".to_string(), "select".to_string()),
    };

    let family = stage_family_for_kind(&kind).to_string();

    TrackStageItem {
        kind: kind.into(),
        label: label.into(),
        family: family.into(),
        enabled: block.enabled,
    }
}

fn build_input_channel_items(
    draft: &TrackDraft,
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
        .tracks
        .iter()
        .enumerate()
        .filter(|(index, track)| {
            track.enabled
                && track.input_device_id.0 == *device_id
                && draft.editing_index != Some(*index)
        })
        .flat_map(|(_, track)| track.input_channels.iter().copied())
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
    draft: &TrackDraft,
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

fn normalized_track_description(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn stop_project_runtime(project_streams: &Rc<RefCell<Option<Vec<Stream>>>>) {
    *project_streams.borrow_mut() = None;
}

fn start_project_runtime(session: &ProjectSession) -> Result<Vec<Stream>> {
    validate_project(&session.project)?;
    let track_sample_rates = resolve_project_track_sample_rates(&session.project)?;
    let runtime_graph = build_runtime_graph(&session.project, &track_sample_rates)?;
    let streams = build_streams_for_project(&session.project, &runtime_graph)?;
    for stream in &streams {
        stream.play()?;
    }
    Ok(streams)
}

fn track_editor_mode(draft: &TrackDraft) -> TrackEditorMode {
    if draft.editing_index.is_some() {
        TrackEditorMode::Edit
    } else {
        TrackEditorMode::Create
    }
}

fn apply_track_editor_labels(window: &AppWindow, draft: &TrackDraft) {
    match track_editor_mode(draft) {
        TrackEditorMode::Create => {
            window.set_track_editor_title("Nova track".into());
            window.set_track_editor_save_label("Criar track".into());
        }
        TrackEditorMode::Edit => {
            window.set_track_editor_title("Configurar track".into());
            window.set_track_editor_save_label("Salvar track".into());
        }
    }
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
