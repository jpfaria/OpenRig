use anyhow::{anyhow, Result};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors,
};
use infra_filesystem::{FilesystemStorage, GuiAudioSettings};
use slint::{Model, ModelRc, VecModel};
use std::rc::Rc;
use ui_openrig::{AppRuntimeMode, InteractionMode, UiRuntimeContext};

slint::include_modules!();

pub fn run_desktop_app(runtime_mode: AppRuntimeMode, interaction_mode: InteractionMode) -> Result<()> {
    let context = UiRuntimeContext::new(runtime_mode, interaction_mode);
    let settings = FilesystemStorage::load_gui_audio_settings()?.unwrap_or_default();

    let window = AppWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    window.set_runtime_mode_label(context.runtime_mode.label().into());
    window.set_interaction_mode_label(context.interaction_mode.label().into());
    window.set_show_audio_setup(context.capabilities.can_select_audio_device);
    window.set_status_message("Selecione os devices e salve a configuração inicial.".into());

    let input_devices = Rc::new(VecModel::from(
        list_input_device_descriptors()?
            .into_iter()
            .map(|device| {
                let name = device.name;
                let selected = settings.input_device_names.iter().any(|saved| saved == &name);
                DeviceSelectionItem {
                    name: name.into(),
                    selected,
                }
            })
            .collect::<Vec<_>>(),
    ));
    let output_devices = Rc::new(VecModel::from(
        list_output_device_descriptors()?
            .into_iter()
            .map(|device| {
                let name = device.name;
                let selected = settings
                    .output_device_names
                    .iter()
                    .any(|saved| saved == &name);
                DeviceSelectionItem {
                    name: name.into(),
                    selected,
                }
            })
            .collect::<Vec<_>>(),
    ));

    window.set_input_devices(ModelRc::from(input_devices.clone()));
    window.set_output_devices(ModelRc::from(output_devices.clone()));
    window.set_sample_rate_text(settings.sample_rate.to_string().into());
    window.set_buffer_size_text(settings.buffer_size_frames.to_string().into());

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
        let weak_window = window.as_weak();
        let input_devices = input_devices.clone();
        let output_devices = output_devices.clone();
        window.on_save_audio_settings(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };

            let sample_rate = match parse_positive_u32(window.get_sample_rate_text().as_str(), "sample_rate") {
                Ok(value) => value,
                Err(error) => {
                    window.set_status_message(error.to_string().into());
                    return;
                }
            };
            let buffer_size_frames =
                match parse_positive_u32(window.get_buffer_size_text().as_str(), "buffer_size_frames") {
                    Ok(value) => value,
                    Err(error) => {
                        window.set_status_message(error.to_string().into());
                        return;
                    }
                };

            let settings = GuiAudioSettings {
                input_device_names: selected_device_names(&input_devices),
                output_device_names: selected_device_names(&output_devices),
                sample_rate,
                buffer_size_frames,
            };

            if !settings.is_complete() {
                window.set_status_message(
                    "Selecione pelo menos um input e um output antes de continuar.".into(),
                );
                return;
            }

            match FilesystemStorage::save_gui_audio_settings(&settings) {
                Ok(()) => window.set_status_message("Configuração salva com sucesso.".into()),
                Err(error) => window.set_status_message(error.to_string().into()),
            }
        });
    }

    window.run().map_err(|error| anyhow!(error.to_string()))
}

fn toggle_device_row(model: &Rc<VecModel<DeviceSelectionItem>>, index: usize, selected: bool) {
    if let Some(mut row) = model.row_data(index) {
        row.selected = selected;
        model.set_row_data(index, row);
    }
}

fn selected_device_names(model: &Rc<VecModel<DeviceSelectionItem>>) -> Vec<String> {
    (0..model.row_count())
        .filter_map(|index| model.row_data(index))
        .filter(|row| row.selected)
        .map(|row| row.name.to_string())
        .collect()
}

fn parse_positive_u32(value: &str, field: &str) -> Result<u32> {
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| anyhow!("'{}' inválido: '{}'", field, value))
}
