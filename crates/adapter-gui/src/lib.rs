use anyhow::{anyhow, Result};
use infra_cpal::{list_input_device_descriptors, list_output_device_descriptors};
use infra_filesystem::{FilesystemStorage, GuiAudioDeviceSettings, GuiAudioSettings};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;
use std::{env, fs, path::PathBuf};
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

#[derive(Debug, Serialize)]
struct ProjectTemplate {
    device_settings: Vec<EmptyYamlItem>,
    tracks: Vec<EmptyYamlItem>,
}

#[derive(Debug, Serialize)]
struct EmptyYamlItem;

pub fn run_desktop_app(runtime_mode: AppRuntimeMode, interaction_mode: InteractionMode) -> Result<()> {
    let context = UiRuntimeContext::new(runtime_mode, interaction_mode);
    let settings = FilesystemStorage::load_gui_audio_settings()?.unwrap_or_default();
    let needs_audio_setup = context.capabilities.can_select_audio_device && !settings.is_complete();
    let project_paths = resolve_project_paths();

    let window = AppWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    window.set_show_project_launcher(true);
    window.set_project_path_label("".into());
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
            window.set_status_message("".into());
            window.set_project_path_label(format!("Projeto: {}", path.display()).into());
            window.set_show_project_launcher(false);
        });
    }

    {
        let weak_window = window.as_weak();
        let project_paths = project_paths.clone();
        window.on_create_project_file(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(path) = FileDialog::new()
                .add_filter("OpenRig Project", &["yaml"])
                .set_title("Criar projeto")
                .set_file_name("project.yaml")
                .save_file()
            else {
                return;
            };
            match create_new_project(&path, &project_paths.default_config_path) {
                Ok(()) => {
                    window.set_status_message("".into());
                    window.set_project_path_label(format!("Projeto: {}", path.display()).into());
                    window.set_show_project_launcher(false);
                }
                Err(error) => {
                    window.set_status_message(error.to_string().into());
                }
            }
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

fn create_new_project(project_path: &PathBuf, default_config_path: &PathBuf) -> Result<()> {
    let project_dir = project_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&project_dir)?;
    let project = ProjectTemplate {
        device_settings: Vec::new(),
        tracks: Vec::new(),
    };
    fs::write(project_path, serde_yaml::to_string(&project)?)?;

    let config_path = project_dir.join("config.yaml");
    if !config_path.exists() {
        let config = if default_config_path.exists() {
            load_app_config(default_config_path)?
        } else {
            AppConfigYaml {
                presets_path: Some(PathBuf::from("./presets")),
            }
        };
        fs::write(&config_path, serde_yaml::to_string(&config)?)?;
    }

    let presets_dir = project_dir.join("presets");
    fs::create_dir_all(presets_dir)?;
    Ok(())
}

fn load_app_config(path: &PathBuf) -> Result<AppConfigYaml> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
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
