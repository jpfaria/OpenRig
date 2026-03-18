use anyhow::Result;
use eframe::egui;
use infra_cpal::{list_input_device_descriptors, list_output_device_descriptors, AudioDeviceDescriptor};
use infra_filesystem::{FilesystemStorage, GuiAudioSettings};
use ui_openrig::{AppRuntimeMode, InteractionMode, OpenRigUi, UiRuntimeContext};

pub struct DesktopGuiApp {
    ui: OpenRigUi,
    wizard: AudioSetupWizard,
}

impl DesktopGuiApp {
    pub fn new(runtime_mode: AppRuntimeMode, interaction_mode: InteractionMode) -> Result<Self> {
        let settings = FilesystemStorage::load_gui_audio_settings()?.unwrap_or_default();
        Self {
            ui: OpenRigUi::new(UiRuntimeContext::new(runtime_mode, interaction_mode)),
            wizard: AudioSetupWizard::new(settings)?,
        }
        .pipe(Ok)
    }
}

impl eframe::App for DesktopGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.ui.context().capabilities.can_select_audio_device && !self.wizard.is_ready() {
            self.wizard.show(ctx);
        } else {
            self.ui.show(ctx);
        }
    }
}

pub fn run_desktop_app(runtime_mode: AppRuntimeMode, interaction_mode: InteractionMode) -> Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "OpenRig",
        native_options,
        Box::new(move |_cc| Ok(Box::new(DesktopGuiApp::new(runtime_mode, interaction_mode)?))),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

struct AudioSetupWizard {
    available_inputs: Vec<AudioDeviceDescriptor>,
    available_outputs: Vec<AudioDeviceDescriptor>,
    selected_input_names: Vec<String>,
    selected_output_names: Vec<String>,
    persisted: bool,
    save_error: Option<String>,
}

impl AudioSetupWizard {
    fn new(settings: GuiAudioSettings) -> Result<Self> {
        let available_inputs = list_input_device_descriptors()?;
        let available_outputs = list_output_device_descriptors()?;
        let persisted = settings.is_complete();

        let selected_input_names = settings
            .input_device_names
            .into_iter()
            .filter(|name| available_inputs.iter().any(|device| device.name == *name))
            .collect();
        let selected_output_names = settings
            .output_device_names
            .into_iter()
            .filter(|name| available_outputs.iter().any(|device| device.name == *name))
            .collect();

        Ok(Self {
            available_inputs,
            available_outputs,
            selected_input_names,
            selected_output_names,
            persisted,
            save_error: None,
        })
    }

    fn has_valid_selection(&self) -> bool {
        !self.selected_input_names.is_empty() && !self.selected_output_names.is_empty()
    }

    fn is_ready(&self) -> bool {
        self.persisted && self.has_valid_selection()
    }

    fn show(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.heading("Configuração inicial");
                ui.add_space(8.0);
                ui.label("Antes de abrir a pedaleira, escolha os devices de entrada e saída.");
            });

            ui.add_space(24.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_max_width(720.0);
                ui.vertical(|ui| {
                    ui.heading("Áudio");
                    ui.add_space(12.0);

                    device_checklist(
                        ui,
                        "Input Devices",
                        &self.available_inputs,
                        &mut self.selected_input_names,
                    );
                    ui.add_space(10.0);
                    device_checklist(
                        ui,
                        "Output Devices",
                        &self.available_outputs,
                        &mut self.selected_output_names,
                    );

                    ui.add_space(16.0);
                    let can_continue = self.has_valid_selection();
                    if ui
                        .add_enabled(can_continue, egui::Button::new("Salvar e continuar"))
                        .clicked()
                    {
                        let result = FilesystemStorage::save_gui_audio_settings(&GuiAudioSettings {
                            input_device_names: self.selected_input_names.clone(),
                            output_device_names: self.selected_output_names.clone(),
                        });
                        match result {
                            Ok(()) => {
                                self.persisted = true;
                                self.save_error = None;
                            }
                            Err(error) => {
                                self.persisted = false;
                                self.save_error = Some(error.to_string());
                            }
                        }
                    }

                    if let Some(error) = &self.save_error {
                        ui.add_space(8.0);
                        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), error);
                    }
                });
            });
        });
    }
}

fn device_checklist(
    ui: &mut egui::Ui,
    label: &str,
    devices: &[AudioDeviceDescriptor],
    selected_names: &mut Vec<String>,
) {
    ui.label(label);
    egui::ScrollArea::vertical()
        .id_salt(label)
        .max_height(140.0)
        .show(ui, |ui| {
            for device in devices {
                let mut checked = selected_names.iter().any(|name| name == &device.name);
                if ui.checkbox(&mut checked, &device.name).changed() {
                    if checked {
                        if !selected_names.iter().any(|name| name == &device.name) {
                            selected_names.push(device.name.clone());
                            selected_names.sort();
                        }
                    } else {
                        selected_names.retain(|name| name != &device.name);
                    }
                }
            }
        });
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}
