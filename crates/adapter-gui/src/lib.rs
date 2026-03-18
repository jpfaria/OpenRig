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
    selected_input_name: Option<String>,
    selected_output_name: Option<String>,
    persisted: bool,
    save_error: Option<String>,
}

impl AudioSetupWizard {
    fn new(settings: GuiAudioSettings) -> Result<Self> {
        let available_inputs = list_input_device_descriptors()?;
        let available_outputs = list_output_device_descriptors()?;
        let persisted = settings.is_complete();

        let selected_input_name = settings
            .input_device_name
            .filter(|name| available_inputs.iter().any(|device| device.name == *name));
        let selected_output_name = settings
            .output_device_name
            .filter(|name| available_outputs.iter().any(|device| device.name == *name));

        Ok(Self {
            available_inputs,
            available_outputs,
            selected_input_name,
            selected_output_name,
            persisted,
            save_error: None,
        })
    }

    fn has_valid_selection(&self) -> bool {
        self.selected_input_name.is_some() && self.selected_output_name.is_some()
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

                    device_combo(
                        ui,
                        "input_device_combo",
                        "Input Device",
                        &self.available_inputs,
                        &mut self.selected_input_name,
                    );
                    ui.add_space(10.0);
                    device_combo(
                        ui,
                        "output_device_combo",
                        "Output Device",
                        &self.available_outputs,
                        &mut self.selected_output_name,
                    );

                    ui.add_space(16.0);
                    let can_continue = self.has_valid_selection();
                    if ui
                        .add_enabled(can_continue, egui::Button::new("Salvar e continuar"))
                        .clicked()
                    {
                        let result = FilesystemStorage::save_gui_audio_settings(&GuiAudioSettings {
                            input_device_name: self.selected_input_name.clone(),
                            output_device_name: self.selected_output_name.clone(),
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

fn device_combo(
    ui: &mut egui::Ui,
    id_salt: &str,
    label: &str,
    devices: &[AudioDeviceDescriptor],
    selected_name: &mut Option<String>,
) {
    ui.label(label);
    let selected_text = selected_name
        .as_deref()
        .unwrap_or("Selecione um device");
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(selected_text)
        .width(480.0)
        .show_ui(ui, |ui| {
            for device in devices {
                ui.selectable_value(selected_name, Some(device.name.clone()), &device.name);
            }
        });
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}
