//! Responsibility: pushes a language change's font to every open window.
//!
//! Each Slint `Window` is a separate root with its own `Locale` global, so a
//! single set on `AppWindow` does not reach the secondary windows — the
//! language section gets a closure that fans the family out to all of them.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::state::ProjectSession;
use crate::{
    AppWindow, ChainEditorWindow, ChainInsertWindow, MetronomeWindow, PluginInfoWindow,
    ProjectSettingsWindow, SpectrumWindow, TunerWindow,
};

pub(crate) struct LanguageWindows<'a> {
    pub window: &'a AppWindow,
    pub project_settings_window: &'a ProjectSettingsWindow,
    pub chain_insert_window: &'a ChainInsertWindow,
    pub tuner_window: &'a TunerWindow,
    pub spectrum_window: &'a SpectrumWindow,
    pub metronome_window: &'a MetronomeWindow,
    pub chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
    pub plugin_info_window: Rc<RefCell<Option<PluginInfoWindow>>>,
}

pub(crate) fn wire(
    windows: LanguageWindows<'_>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
) {
    use slint::Global;
    let weak_app = windows.window.as_weak();
    let weak_proj = windows.project_settings_window.as_weak();
    let weak_chain_insert = windows.chain_insert_window.as_weak();
    let weak_tuner = windows.tuner_window.as_weak();
    let weak_spectrum = windows.spectrum_window.as_weak();
    let weak_metronome = windows.metronome_window.as_weak();
    let chain_editor_window_for_apply = windows.chain_editor_window.clone();
    let plugin_info_window_for_apply = windows.plugin_info_window.clone();
    let apply_font_to_all = move |font: &str| {
        let f = || -> slint::SharedString { font.into() };
        if let Some(w) = weak_app.upgrade() {
            crate::Locale::get(&w).set_font_family(f());
        }
        if let Some(w) = weak_proj.upgrade() {
            crate::Locale::get(&w).set_font_family(f());
        }
        if let Some(w) = weak_chain_insert.upgrade() {
            crate::Locale::get(&w).set_font_family(f());
        }
        if let Some(w) = weak_tuner.upgrade() {
            crate::Locale::get(&w).set_font_family(f());
        }
        if let Some(w) = weak_spectrum.upgrade() {
            crate::Locale::get(&w).set_font_family(f());
        }
        if let Some(w) = weak_metronome.upgrade() {
            crate::Locale::get(&w).set_font_family(f());
        }
        if let Some(w) = chain_editor_window_for_apply.borrow().as_ref() {
            crate::Locale::get(w).set_font_family(f());
        }
        if let Some(w) = plugin_info_window_for_apply.borrow().as_ref() {
            crate::Locale::get(w).set_font_family(f());
        }
    };
    crate::settings::language::wire(
        windows.window,
        windows.project_settings_window,
        project_session,
        apply_font_to_all,
    );
}
