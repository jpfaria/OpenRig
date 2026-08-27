//! Responsibility: describes the UI context the plugin build runs under.
use ui_openrig::{AppRuntimeMode, InteractionMode, UiRuntimeContext};

pub fn plugin_ui_context() -> UiRuntimeContext {
    UiRuntimeContext::new(AppRuntimeMode::Vst3Plugin, InteractionMode::Mouse)
}
