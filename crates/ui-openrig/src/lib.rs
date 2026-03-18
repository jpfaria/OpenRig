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
