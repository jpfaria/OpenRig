//! Responsibility: creates every window the desktop app opens with.
//!
//! Each Slint `Window` is its own root with its own `Locale` global, so the
//! boot font has to be set on each one as it is constructed. The bundled
//! translation can only be selected once a component exists, which is why it
//! is applied here rather than before `AppWindow::new`.

use anyhow::{anyhow, Result};
use infra_filesystem::FilesystemStorage;
use slint::{ComponentHandle, Global};
use std::cell::RefCell;
use std::rc::Rc;

use crate::{
    AppWindow, ChainEditorWindow, ChainInsertWindow, ChainPortWindow, MetronomeWindow,
    PluginInfoWindow, ProjectSettingsWindow, SpectrumWindow, TunerWindow,
};

pub(crate) struct DesktopWindows {
    pub window: AppWindow,
    pub project_settings_window: ProjectSettingsWindow,
    pub chain_insert_window: ChainInsertWindow,
    pub chain_port_window: ChainPortWindow,
    pub tuner_window: TunerWindow,
    pub spectrum_window: SpectrumWindow,
    pub metronome_window: MetronomeWindow,
    /// Built on demand by the chain editor's open callback.
    pub chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
    /// Built on demand when a plugin's info panel is opened.
    pub plugin_info_window: Rc<RefCell<Option<PluginInfoWindow>>>,
}

pub(crate) fn create() -> Result<DesktopWindows> {
    let window = AppWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    let boot_font = crate::i18n::font_for_persisted_runtime();
    crate::Locale::get(&window).set_font_family(boot_font.into());
    // Slint's select_bundled_translation requires at least one component to
    // exist before it can resolve the bundled language list.
    let persisted_language = FilesystemStorage::load_gui_audio_settings()
        .ok()
        .flatten()
        .and_then(|s| s.language);
    crate::i18n::apply_bundled_translation(persisted_language.as_deref());
    window
        .window()
        .set_size(slint::WindowSize::Logical(slint::LogicalSize {
            width: 1100.0,
            height: 620.0,
        }));

    let project_settings_window =
        ProjectSettingsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&project_settings_window).set_font_family(boot_font.into());

    let chain_insert_window =
        ChainInsertWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&chain_insert_window).set_font_family(boot_font.into());

    // #85 — the mid-chain I/O port editor.
    let chain_port_window = ChainPortWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&chain_port_window).set_font_family(boot_font.into());

    let tuner_window = TunerWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&tuner_window).set_font_family(boot_font.into());

    let spectrum_window = SpectrumWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&spectrum_window).set_font_family(boot_font.into());

    let metronome_window = MetronomeWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&metronome_window).set_font_family(boot_font.into());

    Ok(DesktopWindows {
        window,
        project_settings_window,
        chain_insert_window,
        chain_port_window,
        tuner_window,
        spectrum_window,
        metronome_window,
        chain_editor_window: Rc::new(RefCell::new(None)),
        plugin_info_window: Rc::new(RefCell::new(None)),
    })
}
