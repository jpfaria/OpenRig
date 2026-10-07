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
    AppWindow, ChainEditorWindow, ChainInsertWindow, ChainPortWindow, DrumsWindow, MetronomeWindow,
    MixerWindow, PlayerWindow, PluginEditorWindow, PluginInfoWindow, PluginsWindow,
    ProjectSettingsWindow, SpectrumWindow, Tone3000Window, TunerWindow,
};

pub(crate) struct DesktopWindows {
    pub window: AppWindow,
    pub project_settings_window: ProjectSettingsWindow,
    pub chain_insert_window: ChainInsertWindow,
    pub chain_port_window: ChainPortWindow,
    pub tuner_window: TunerWindow,
    pub spectrum_window: SpectrumWindow,
    pub metronome_window: MetronomeWindow,
    pub mixer_window: MixerWindow,
    pub player_window: PlayerWindow,
    pub drums_window: DrumsWindow,
    /// #879 — the TONE3000 browser.
    pub tone3000_window: Tone3000Window,
    /// The catalog of the plugins the user installed.
    pub plugins_window: PluginsWindow,
    /// The capture grid of one plugin.
    pub plugin_editor_window: PluginEditorWindow,
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
        .set_size(crate::main_window_size::initial_size());

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

    // #1007 — the global mixer.
    let mixer_window = MixerWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&mixer_window).set_font_family(boot_font.into());

    let player_window = PlayerWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&player_window).set_font_family(boot_font.into());
    let drums_window = DrumsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&drums_window).set_font_family(boot_font.into());
    let tone3000_window = Tone3000Window::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&tone3000_window).set_font_family(boot_font.into());
    let plugins_window = PluginsWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&plugins_window).set_font_family(boot_font.into());
    let plugin_editor_window =
        PluginEditorWindow::new().map_err(|error| anyhow!(error.to_string()))?;
    crate::Locale::get(&plugin_editor_window).set_font_family(boot_font.into());

    Ok(DesktopWindows {
        window,
        project_settings_window,
        chain_insert_window,
        chain_port_window,
        tuner_window,
        spectrum_window,
        metronome_window,
        mixer_window,
        player_window,
        drums_window,
        tone3000_window,
        plugins_window,
        plugin_editor_window,
        chain_editor_window: Rc::new(RefCell::new(None)),
        plugin_info_window: Rc::new(RefCell::new(None)),
    })
}
