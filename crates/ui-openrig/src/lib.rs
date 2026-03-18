use egui::{Align, Color32, Layout, RichText, Sense, Stroke, Ui, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppRuntimeMode {
    Standalone,
    Pedalboard,
    Controller,
    Vst3Plugin,
}

impl AppRuntimeMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Standalone => "Standalone",
            Self::Pedalboard => "Pedaleira",
            Self::Controller => "Controlador",
            Self::Vst3Plugin => "VST3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    Mouse,
    Touch,
}

impl InteractionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mouse => "Mouse",
            Self::Touch => "Touch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiCapabilities {
    pub uses_local_audio: bool,
    pub can_select_audio_device: bool,
    pub can_select_remote_host: bool,
    pub hosted_by_daw: bool,
    pub touch_optimized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiRuntimeContext {
    pub runtime_mode: AppRuntimeMode,
    pub interaction_mode: InteractionMode,
    pub capabilities: UiCapabilities,
}

impl UiRuntimeContext {
    pub fn new(runtime_mode: AppRuntimeMode, interaction_mode: InteractionMode) -> Self {
        let capabilities = match runtime_mode {
            AppRuntimeMode::Standalone => UiCapabilities {
                uses_local_audio: true,
                can_select_audio_device: true,
                can_select_remote_host: false,
                hosted_by_daw: false,
                touch_optimized: matches!(interaction_mode, InteractionMode::Touch),
            },
            AppRuntimeMode::Pedalboard => UiCapabilities {
                uses_local_audio: true,
                can_select_audio_device: true,
                can_select_remote_host: false,
                hosted_by_daw: false,
                touch_optimized: true,
            },
            AppRuntimeMode::Controller => UiCapabilities {
                uses_local_audio: false,
                can_select_audio_device: false,
                can_select_remote_host: true,
                hosted_by_daw: false,
                touch_optimized: true,
            },
            AppRuntimeMode::Vst3Plugin => UiCapabilities {
                uses_local_audio: false,
                can_select_audio_device: false,
                can_select_remote_host: false,
                hosted_by_daw: true,
                touch_optimized: false,
            },
        };

        Self {
            runtime_mode,
            interaction_mode,
            capabilities,
        }
    }
}

pub struct OpenRigUi {
    context: UiRuntimeContext,
}

impl OpenRigUi {
    pub fn new(context: UiRuntimeContext) -> Self {
        Self { context }
    }

    pub fn context(&self) -> UiRuntimeContext {
        self.context
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        apply_theme(ctx, self.context);

        egui::TopBottomPanel::top("top_bar")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("OpenRig").strong().size(28.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        capability_chip(ui, self.context.interaction_mode.label(), Color32::from_rgb(44, 169, 108));
                        capability_chip(ui, self.context.runtime_mode.label(), Color32::from_rgb(220, 144, 55));
                    });
                });
                ui.add_space(6.0);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            hero(ui, self.context);
            ui.add_space(14.0);
            ui.columns(2, |columns| {
                left_column(&mut columns[0], self.context);
                right_column(&mut columns[1], self.context);
            });
        });
    }
}

fn apply_theme(ctx: &egui::Context, context: UiRuntimeContext) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = if context.capabilities.touch_optimized {
        Vec2::new(14.0, 14.0)
    } else {
        Vec2::new(10.0, 10.0)
    };
    style.spacing.button_padding = if context.capabilities.touch_optimized {
        Vec2::new(16.0, 14.0)
    } else {
        Vec2::new(10.0, 8.0)
    };
    ctx.set_style(style);
}

fn hero(ui: &mut Ui, context: UiRuntimeContext) {
    card(ui, "Modo atual", |ui| {
        ui.label(
            RichText::new(match context.runtime_mode {
                AppRuntimeMode::Standalone => "Host local com interface de áudio e edição completa.",
                AppRuntimeMode::Pedalboard => "Pedaleira física com áudio local e interface otimizada para toque.",
                AppRuntimeMode::Controller => "Controlador remoto sem interface de áudio local.",
                AppRuntimeMode::Vst3Plugin => "Plugin dentro da DAW usando o áudio hospedado pelo host.",
            })
            .size(18.0),
        );
    });
}

fn left_column(ui: &mut Ui, context: UiRuntimeContext) {
    card(ui, "Sessão", |ui| {
        ui.label(format!("Modo: {}", context.runtime_mode.label()));
        ui.label(format!("Interação: {}", context.interaction_mode.label()));
        ui.label(format!(
            "Áudio local: {}",
            yes_no(context.capabilities.uses_local_audio)
        ));
        ui.label(format!(
            "Hospedado em DAW: {}",
            yes_no(context.capabilities.hosted_by_daw)
        ));
    });

    card(ui, "Fluxo de conexão", |ui| {
        if context.capabilities.can_select_audio_device {
            ui.label("Selecionar interface de áudio local");
        }
        if context.capabilities.can_select_remote_host {
            ui.label("Selecionar pedaleira ou servidor host");
        }
        if context.capabilities.hosted_by_daw {
            ui.label("Receber áudio e automação do host VST3");
        }
        if !context.capabilities.can_select_audio_device
            && !context.capabilities.can_select_remote_host
            && !context.capabilities.hosted_by_daw
        {
            ui.label("Modo ainda sem conectores especiais.");
        }
    });
}

fn right_column(ui: &mut Ui, context: UiRuntimeContext) {
    card(ui, "Capacidades", |ui| {
        capability_row(ui, "Selecionar interface de áudio", context.capabilities.can_select_audio_device);
        capability_row(ui, "Conectar em host remoto", context.capabilities.can_select_remote_host);
        capability_row(ui, "Layout touch", context.capabilities.touch_optimized);
        capability_row(ui, "Executa áudio local", context.capabilities.uses_local_audio);
    });

    card(ui, "Próximo uso", |ui| {
        match context.runtime_mode {
            AppRuntimeMode::Standalone => ui.label("Base para app desktop em macOS, Windows e Linux."),
            AppRuntimeMode::Pedalboard => ui.label("Base para a GUI da pedaleira física em Linux."),
            AppRuntimeMode::Controller => ui.label("Base para o controlador dedicado que conecta em um host."),
            AppRuntimeMode::Vst3Plugin => ui.label("Base para a mesma GUI embutida no editor do plugin."),
        };
    });
}

fn card(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::group(ui.style())
        .stroke(Stroke::new(1.0, Color32::from_gray(60)))
        .corner_radius(12.0)
        .fill(Color32::from_rgb(18, 20, 24))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.heading(RichText::new(title).size(18.0));
                ui.add_space(8.0);
                add_contents(ui);
            });
        });
}

fn capability_row(ui: &mut Ui, label: &str, enabled: bool) {
    ui.horizontal(|ui| {
        let dot = if enabled { "●" } else { "○" };
        let color = if enabled {
            Color32::from_rgb(44, 169, 108)
        } else {
            Color32::from_gray(120)
        };
        ui.label(RichText::new(dot).color(color));
        ui.label(label);
    });
}

fn capability_chip(ui: &mut Ui, label: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(108.0, 28.0), Sense::hover());
    ui.painter().rect_filled(rect, 999.0, color);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(14.0),
        Color32::BLACK,
    );
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "Sim"
    } else {
        "Não"
    }
}
