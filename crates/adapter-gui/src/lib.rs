use anyhow::Result;
use eframe::egui;
use ui_openrig::{AppRuntimeMode, InteractionMode, OpenRigUi, UiRuntimeContext};

pub struct DesktopGuiApp {
    ui: OpenRigUi,
}

impl DesktopGuiApp {
    pub fn new(runtime_mode: AppRuntimeMode, interaction_mode: InteractionMode) -> Self {
        Self {
            ui: OpenRigUi::new(UiRuntimeContext::new(runtime_mode, interaction_mode)),
        }
    }
}

impl eframe::App for DesktopGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui.show(ctx);
    }
}

pub fn run_desktop_app(runtime_mode: AppRuntimeMode, interaction_mode: InteractionMode) -> Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "OpenRig",
        native_options,
        Box::new(move |_cc| Ok(Box::new(DesktopGuiApp::new(runtime_mode, interaction_mode)))),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}
