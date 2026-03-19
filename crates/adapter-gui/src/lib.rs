use anyhow::{anyhow, Result};
use application::validate::validate_setup;
use cpal::{traits::StreamTrait, Stream};
use domain::ids::{DeviceId, TrackId};
use engine::runtime::build_runtime_graph;
use infra_cpal::{
    build_streams_for_setup, list_input_device_descriptors, list_output_device_descriptors,
    AudioDeviceDescriptor,
};
use infra_filesystem::{FilesystemStorage, GuiAudioDeviceSettings, GuiAudioSettings};
use infra_yaml::{load_setup_preset_file, save_setup_preset_file, serialize_audio_blocks, TrackStagesPreset, YamlSetupRepository};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use std::{cell::RefCell, env, fs, path::PathBuf};
use setup::device::DeviceSettings;
use setup::block::{AudioBlock, AudioBlockKind, CoreBlockKind};
use setup::setup::Setup;
use setup::track::{Track, TrackOutputMixdown};
use ui_openrig::{AppRuntimeMode, InteractionMode, UiRuntimeContext};

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
    setup: Setup,
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

pub fn run_desktop_app(runtime_mode: AppRuntimeMode, interaction_mode: InteractionMode) -> Result<()> {
    let context = UiRuntimeContext::new(runtime_mode, interaction_mode);
    let settings = FilesystemStorage::load_gui_audio_settings()?.unwrap_or_default();
    let needs_audio_setup = context.capabilities.can_select_audio_device && !settings.is_complete();
    let project_paths = resolve_project_paths();
    let project_session = Rc::new(RefCell::new(None::<ProjectSession>));
    let track_draft = Rc::new(RefCell::new(None::<TrackDraft>));
    let project_streams = Rc::new(RefCell::new(None::<Vec<Stream>>));
    let audio_settings_mode = Rc::new(RefCell::new(AudioSettingsMode::Gui));
    let input_track_devices = Rc::new(list_input_device_descriptors()?);
    let output_track_devices = Rc::new(list_output_device_descriptors()?);

    let window = AppWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    window.set_show_project_launcher(true);
    window.set_show_project_tracks(false);
    window.set_show_track_editor(false);
    window.set_show_project_settings(false);
    window.set_project_running(false);
    window.set_project_path_label("".into());
    window.set_project_title("Projeto".into());
    window.set_project_name_draft("".into());
    window.set_track_editor_title("Nova track".into());
    window.set_track_editor_save_label("Criar track".into());
    window.set_runtime_mode_label(context.runtime_mode.label().into());
    window.set_interaction_mode_label(context.interaction_mode.label().into());
    window.set_touch_optimized(context.capabilities.touch_optimized);
    window.set_show_audio_setup(needs_audio_setup);
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
    window.set_track_draft_name("".into());

    {
        let input_devices = input_devices.clone();
        window.on_toggle_input_device(move |index, selected| {
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
                        window.set_show_audio_setup(false);
                    }
                    Err(error) => window.set_status_message(error.to_string().into()),
                },
                AudioSettingsMode::Project => {
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        window.set_status_message("Nenhum projeto carregado.".into());
                        return;
                    };
                    session.setup.device_settings = merge_device_settings(
                        settings.input_devices,
                        settings.output_devices,
                    );
                    let was_running = project_streams.borrow().is_some();
                    if was_running {
                        stop_project_runtime(&project_streams);
                        window.set_project_running(false);
                    }
                    replace_project_tracks(&project_tracks, &session.setup);
                    window.set_project_title(
                        project_title_for_path(session.project_path.as_ref(), &session.setup).into(),
                    );
                    window.set_status_message(if was_running {
                        restart_message("Configuração do projeto atualizada.")
                    } else {
                        "Configuração do projeto atualizada.".into()
                    });
                    window.set_show_project_tracks(true);
                    window.set_show_track_editor(false);
                    window.set_show_project_settings(false);
                }
            }
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
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
                    stop_project_runtime(&project_streams);
                    replace_project_tracks(&project_tracks, &session.setup);
                    let title = project_title_for_path(Some(&path), &session.setup);
                    *project_session.borrow_mut() = Some(session);
                    window.set_project_running(false);
                    window.set_status_message("".into());
                    window.set_project_title(title.into());
                    window.set_project_name_draft(
                        project_session
                            .borrow()
                            .as_ref()
                            .and_then(|session| session.setup.name.clone())
                            .unwrap_or_default()
                            .into(),
                    );
                    window.set_project_path_label(format!("Projeto: {}", path.display()).into());
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
        window.on_create_project_file(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            stop_project_runtime(&project_streams);
            let session = create_new_project_session(&project_paths.default_config_path);
            replace_project_tracks(&project_tracks, &session.setup);
            *project_session.borrow_mut() = Some(session);
            window.set_project_running(false);
            window.set_status_message("".into());
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
        let project_session = project_session.clone();
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
                    window.set_project_title(project_title_for_path(Some(&project_path), &session.setup).into());
                    window.set_project_name_draft(session.setup.name.clone().unwrap_or_default().into());
                    window.set_project_path_label(format!("Projeto: {}", project_path.display()).into());
                    window.set_status_message("Projeto salvo.".into());
                }
                Err(error) => {
                    window.set_status_message(error.to_string().into());
                }
            }
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let input_devices = input_devices.clone();
        let output_devices = output_devices.clone();
        let audio_settings_mode = audio_settings_mode.clone();
        window.on_configure_project(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };

            set_device_selection_from_project(&input_devices, &session.setup.device_settings);
            set_device_selection_from_project(&output_devices, &session.setup.device_settings);
            *audio_settings_mode.borrow_mut() = AudioSettingsMode::Project;
            window.set_project_name_draft(session.setup.name.clone().unwrap_or_default().into());
            window.set_status_message("".into());
            window.set_show_project_launcher(false);
            window.set_show_project_tracks(false);
            window.set_show_track_editor(false);
            window.set_show_project_settings(true);
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        window.on_update_project_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            window.set_project_name_draft(value.clone());
            if let Some(session) = project_session.borrow_mut().as_mut() {
                let trimmed = value.trim();
                session.setup.name = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
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
            window.set_show_project_tracks(true);
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
            let Some(track) = session.setup.tracks.get(index as usize) else {
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
                    if let Some(track) = session.setup.tracks.get_mut(index as usize) {
                        track.blocks = preset.blocks;
                        let was_running = project_streams.borrow().is_some();
                        if was_running {
                            stop_project_runtime(&project_streams);
                            window.set_project_running(false);
                        }
                        replace_project_tracks(&project_tracks, &session.setup);
                        window.set_status_message(if was_running {
                            restart_message("Preset aplicado na track.")
                        } else {
                            "Preset aplicado na track.".into()
                        });
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
        window.on_back_to_launcher(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            stop_project_runtime(&project_streams);
            *project_session.borrow_mut() = None;
            replace_project_tracks(&project_tracks, &Setup {
                name: None,
                device_settings: Vec::new(),
                tracks: Vec::new(),
            });
            window.set_status_message("".into());
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
        window.on_add_track(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let borrow = project_session.borrow();
            let Some(session) = borrow.as_ref() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let draft = create_track_draft(&session.setup, &input_track_devices, &output_track_devices);
            *track_draft.borrow_mut() = Some(draft.clone());
            apply_track_editor_labels(&window, &draft);
            replace_channel_options(
                &track_input_channels,
                build_input_channel_items(&draft, &session.setup, &input_track_devices),
            );
            replace_channel_options(
                &track_output_channels,
                build_output_channel_items(&draft, &output_track_devices),
            );
            window.set_track_draft_name(draft.name.clone().into());
            window.set_selected_track_input_device_index(selected_device_index(
                &input_track_devices,
                draft.input_device_id.as_deref(),
            ));
            window.set_selected_track_output_device_index(selected_device_index(
                &output_track_devices,
                draft.output_device_id.as_deref(),
            ));
            window.set_status_message("".into());
            window.set_show_project_tracks(false);
            window.set_show_track_editor(true);
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
        window.on_configure_track(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let Some(track) = session.setup.tracks.get(index as usize) else {
                window.set_status_message("Track inválida.".into());
                return;
            };
            let draft = track_draft_from_track(index as usize, track);
            replace_channel_options(
                &track_input_channels,
                build_input_channel_items(&draft, &session.setup, &input_track_devices),
            );
            replace_channel_options(
                &track_output_channels,
                build_output_channel_items(&draft, &output_track_devices),
            );
            window.set_track_draft_name(draft.name.clone().into());
            window.set_selected_track_input_device_index(selected_device_index(
                &input_track_devices,
                draft.input_device_id.as_deref(),
            ));
            window.set_selected_track_output_device_index(selected_device_index(
                &output_track_devices,
                draft.output_device_id.as_deref(),
            ));
            *track_draft.borrow_mut() = Some(draft);
            if let Some(draft) = track_draft.borrow().as_ref() {
                apply_track_editor_labels(&window, draft);
            }
            window.set_status_message("".into());
            window.set_show_project_settings(false);
            window.set_show_project_tracks(false);
            window.set_show_track_editor(true);
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
                    build_input_channel_items(draft, &session.setup, &input_track_devices),
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
        let track_draft = track_draft.clone();
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
            replace_channel_options(
                &track_output_channels,
                build_output_channel_items(draft, &output_track_devices),
            );
            window.set_selected_track_output_device_index(selected_device_index(
                &output_track_devices,
                draft.output_device_id.as_deref(),
            ));
        });
    }

    {
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let input_track_devices = input_track_devices.clone();
        let track_input_channels = track_input_channels.clone();
        window.on_toggle_track_input_channel(move |index, selected| {
            let mut draft_borrow = track_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let channel = index as usize;
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
                    build_input_channel_items(draft, &session.setup, &input_track_devices),
                );
            }
        });
    }

    {
        let track_draft = track_draft.clone();
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
            replace_channel_options(
                &track_output_channels,
                build_output_channel_items(draft, &output_track_devices),
            );
        });
    }

    {
        let weak_window = window.as_weak();
        let track_draft = track_draft.clone();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
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
            let existing_track = editing_index.and_then(|index| session.setup.tracks.get(index).cloned());
            let track = Track {
                id: existing_track
                    .as_ref()
                    .map(|track| track.id.clone())
                    .unwrap_or_else(|| TrackId(format!("track:{}", session.setup.tracks.len()))),
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
                if let Some(current) = session.setup.tracks.get_mut(index) {
                    *current = track;
                }
            } else {
                session.setup.tracks.push(track);
            }
            let was_running = project_streams.borrow().is_some();
            if was_running {
                stop_project_runtime(&project_streams);
                window.set_project_running(false);
            }
            replace_project_tracks(&project_tracks, &session.setup);
            *track_draft.borrow_mut() = None;
            window.set_status_message(if was_running {
                restart_message("Track atualizada.")
            } else {
                "".into()
            });
            window.set_show_track_editor(false);
            window.set_show_project_tracks(true);
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
            window.set_show_project_tracks(true);
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        let project_streams = project_streams.clone();
        window.on_toggle_track_enabled(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let Some(track) = session.setup.tracks.get_mut(index as usize) else {
                window.set_status_message("Track inválida.".into());
                return;
            };
            track.enabled = !track.enabled;
            let was_running = project_streams.borrow().is_some();
            if was_running {
                stop_project_runtime(&project_streams);
                window.set_project_running(false);
            }
            replace_project_tracks(&project_tracks, &session.setup);
            window.set_status_message(if was_running {
                restart_message("Track atualizada.")
            } else {
                "Track atualizada.".into()
            });
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
                    window.set_status_message("Projeto em execução.".into());
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
            window.set_status_message("Projeto parado.".into());
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
    let setup = Setup {
        name: None,
        device_settings: Vec::new(),
        tracks: Vec::new(),
    };
    ProjectSession {
        setup,
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
    let setup = YamlSetupRepository { path: project_path.clone() }.load_current_setup()?;
    Ok(ProjectSession {
        setup,
        project_path: Some(project_path.clone()),
        config_path: Some(config_path.clone()),
        presets_path: project_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(presets_path),
    })
}

fn replace_project_tracks(model: &Rc<VecModel<ProjectTrackItem>>, setup: &Setup) {
    let items = setup
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| ProjectTrackItem {
            title: track
                .description
                .clone()
                .unwrap_or_else(|| format!("Track {}", index + 1))
                .into(),
            subtitle: format!(
                "Entrada {} -> Saída {}",
                channels_label(&track.input_channels),
                channels_label(&track.output_channels),
            )
            .into(),
            enabled: track.enabled,
            stages: ModelRc::from(Rc::new(VecModel::from(
                track.blocks.iter().map(track_stage_item_from_block).collect::<Vec<_>>(),
            ))),
        })
        .collect::<Vec<_>>();
    model.set_vec(items);
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

fn save_project_session(session: &ProjectSession, project_path: &PathBuf) -> Result<()> {
    let parent_dir = project_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent_dir)?;

    let project = ProjectYaml {
        name: session
            .setup
            .name
            .as_ref()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty()),
        device_settings: session
            .setup
            .device_settings
            .iter()
            .map(|setting| ProjectDeviceSettingsYaml {
                device_id: setting.device_id.0.clone(),
                sample_rate: setting.sample_rate,
                buffer_size_frames: setting.buffer_size_frames,
            })
            .collect(),
        tracks: session
            .setup
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
    };

    fs::write(project_path, serde_yaml::to_string(&project)?)?;

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
    let preset = TrackStagesPreset {
        id: preset_id_from_path(path)?,
        name: track.description.clone(),
        blocks: track.blocks.clone(),
    };
    save_setup_preset_file(path, &preset)
}

fn load_preset_file(path: &PathBuf) -> Result<TrackStagesPreset> {
    load_setup_preset_file(path)
}

fn preset_id_from_path(path: &PathBuf) -> Result<String> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow!("arquivo de preset inválido"))
}

fn project_title_for_path(project_path: Option<&PathBuf>, setup: &Setup) -> String {
    if let Some(name) = setup.name.as_ref().map(|name| name.trim()).filter(|name| !name.is_empty()) {
        return name.to_string();
    }
    project_path
        .and_then(|path| path.file_stem())
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| {
            if setup.tracks.is_empty() {
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
    setup: &Setup,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) -> TrackDraft {
    TrackDraft {
        editing_index: None,
        name: format!("Track {}", setup.tracks.len() + 1),
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

fn channels_label(channels: &[usize]) -> String {
    channels
        .iter()
        .map(|channel| (channel + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn track_stage_item_from_block(block: &AudioBlock) -> TrackStageItem {
    let (kind, label) = match &block.kind {
        AudioBlockKind::Nam(stage) => ("nam".to_string(), stage.model.clone()),
        AudioBlockKind::Core(core) => match &core.kind {
            CoreBlockKind::AmpHead(stage) => ("amp_head".to_string(), stage.model.clone()),
            CoreBlockKind::AmpCombo(stage) => ("amp_combo".to_string(), stage.model.clone()),
            CoreBlockKind::FullRig(stage) => ("full_rig".to_string(), stage.model.clone()),
            CoreBlockKind::Drive(stage) => ("drive".to_string(), stage.model.clone()),
            CoreBlockKind::Compressor(stage) => ("compressor".to_string(), stage.model.clone()),
            CoreBlockKind::Gate(stage) => ("gate".to_string(), stage.model.clone()),
            CoreBlockKind::Eq(stage) => ("eq".to_string(), stage.model.clone()),
            CoreBlockKind::Tremolo(stage) => ("tremolo".to_string(), stage.model.clone()),
            CoreBlockKind::Delay(stage) => ("delay".to_string(), stage.model.clone()),
            CoreBlockKind::Reverb(stage) => ("reverb".to_string(), stage.model.clone()),
            CoreBlockKind::Tuner(stage) => ("tuner".to_string(), stage.model.clone()),
            _ => ("core".to_string(), "stage".to_string()),
        },
        AudioBlockKind::Select(_) => ("core".to_string(), "select".to_string()),
        AudioBlockKind::CoreNam(_) => ("nam".to_string(), "core nam".to_string()),
    };

    TrackStageItem {
        kind: kind.into(),
        label: label.into(),
        enabled: block.enabled,
    }
}

fn build_input_channel_items(
    draft: &TrackDraft,
    setup: &Setup,
    input_devices: &[AudioDeviceDescriptor],
) -> Vec<ChannelOptionItem> {
    let Some(device_id) = draft.input_device_id.as_ref() else {
        return Vec::new();
    };
    let Some(device) = input_devices.iter().find(|device| &device.id == device_id) else {
        return Vec::new();
    };
    let used_channels = setup
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

fn restart_message(base: &str) -> SharedString {
    format!("{base} Projeto parado. Clique em play para reiniciar.").into()
}

fn start_project_runtime(session: &ProjectSession) -> Result<Vec<Stream>> {
    validate_setup(&session.setup)?;
    let runtime_graph = build_runtime_graph(&session.setup)?;
    let streams = build_streams_for_setup(&session.setup, &runtime_graph)?;
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
