use anyhow::{anyhow, Result};
use infra_cpal::{list_input_device_descriptors, list_output_device_descriptors};
use infra_filesystem::{FilesystemStorage, GuiAudioDeviceSettings, GuiAudioSettings};
use infra_yaml::YamlSetupRepository;
use ports::SetupRepository;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;
use std::{cell::RefCell, env, fs, path::PathBuf};
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
}

pub fn run_desktop_app(runtime_mode: AppRuntimeMode, interaction_mode: InteractionMode) -> Result<()> {
    let context = UiRuntimeContext::new(runtime_mode, interaction_mode);
    let settings = FilesystemStorage::load_gui_audio_settings()?.unwrap_or_default();
    let needs_audio_setup = context.capabilities.can_select_audio_device && !settings.is_complete();
    let project_paths = resolve_project_paths();
    let project_session = Rc::new(RefCell::new(None::<ProjectSession>));

    let window = AppWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    window.set_show_project_launcher(true);
    window.set_show_project_tracks(false);
    window.set_project_path_label("".into());
    window.set_project_title("Projeto".into());
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

            match FilesystemStorage::save_gui_audio_settings(&settings) {
                Ok(()) => {
                    window.set_status_message("".into());
                    window.set_show_audio_setup(false);
                }
                Err(error) => window.set_status_message(error.to_string().into()),
            }
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
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
                    replace_project_tracks(&project_tracks, &session.setup);
                    let title = project_title_for_path(Some(&path), &session.setup);
                    *project_session.borrow_mut() = Some(session);
                    window.set_status_message("".into());
                    window.set_project_title(title.into());
                    window.set_project_path_label(format!("Projeto: {}", path.display()).into());
                    window.set_show_project_launcher(false);
                    window.set_show_project_tracks(true);
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
        window.on_create_project_file(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let session = create_new_project_session(&project_paths.default_config_path);
            replace_project_tracks(&project_tracks, &session.setup);
            *project_session.borrow_mut() = Some(session);
            window.set_status_message("".into());
            window.set_project_title("Novo Projeto".into());
            window.set_project_path_label("Projeto em memória".into());
            window.set_show_project_launcher(false);
            window.set_show_project_tracks(true);
        });
    }

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let project_tracks = project_tracks.clone();
        window.on_add_track(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut borrow = project_session.borrow_mut();
            let Some(session) = borrow.as_mut() else {
                window.set_status_message("Nenhum projeto carregado.".into());
                return;
            };
            let next_index = session.setup.tracks.len() + 1;
            session.setup.tracks.push(Track {
                id: domain::ids::TrackId(format!("track:{}", next_index - 1)),
                enabled: true,
                input_device_id: domain::ids::DeviceId(String::new()),
                input_channels: Vec::new(),
                output_device_id: domain::ids::DeviceId(String::new()),
                output_channels: Vec::new(),
                preset_id: domain::ids::PresetId(String::new()),
                output_mixdown: TrackOutputMixdown::Average,
            });
            replace_project_tracks(&project_tracks, &session.setup);
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
    let _config = if default_config_path.exists() {
        load_app_config(default_config_path).unwrap_or_default()
    } else {
        AppConfigYaml {
            presets_path: Some(PathBuf::from("./presets")),
        }
    };
    let setup = Setup {
        device_settings: Vec::new(),
        presets: Vec::new(),
        tracks: Vec::new(),
    };
    ProjectSession {
        setup,
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
    let setup = YamlSetupRepository {
        path: project_path.clone(),
        presets_path_override: config.presets_path,
    }
    .load_current_setup()?;
    Ok(ProjectSession {
        setup,
    })
}

fn replace_project_tracks(model: &Rc<VecModel<ProjectTrackItem>>, setup: &Setup) {
    let items = setup
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| ProjectTrackItem {
            title: format!("Track {}", index + 1).into(),
            subtitle: if track.preset_id.0.is_empty() {
                "sem preset".into()
            } else {
                track.preset_id.0.clone().into()
            },
            enabled: track.enabled,
        })
        .collect::<Vec<_>>();
    model.set_vec(items);
}

fn project_title_for_path(project_path: Option<&PathBuf>, setup: &Setup) -> String {
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
