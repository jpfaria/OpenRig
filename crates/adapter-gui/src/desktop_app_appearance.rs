//! Responsibility: paints the chosen colour scheme on every window.
//!
//! Each Slint `Window` is its own root with its own `Theme` global, so a mode
//! set on `AppWindow` never reaches the tuner or the mixer. The Appearance
//! section gets a closure that sets it on all of them; a window built later
//! reads the same choice as it opens (`settings::appearance_current`).

use std::cell::RefCell;
use std::rc::Rc;

use infra_filesystem::{AppConfig, Appearance};
use slint::{ComponentHandle, Weak};

use crate::desktop_app_windows::DesktopWindows;
use crate::settings::appearance_current::{apply, remember};
use crate::state::ProjectSession;
use crate::{
    AppWindow, ChainEditorWindow, ChainInsertWindow, ChainPortWindow, DrumsWindow, MetronomeWindow,
    MixerWindow, PlayerWindow, PluginEditorWindow, PluginInfoWindow, PluginsWindow,
    ProjectSettingsWindow, SpectrumWindow, Tone3000Window, TunerWindow,
};

struct SchemeWindows {
    app: Weak<AppWindow>,
    project_settings: Weak<ProjectSettingsWindow>,
    chain_insert: Weak<ChainInsertWindow>,
    chain_port: Weak<ChainPortWindow>,
    tuner: Weak<TunerWindow>,
    spectrum: Weak<SpectrumWindow>,
    metronome: Weak<MetronomeWindow>,
    mixer: Weak<MixerWindow>,
    player: Weak<PlayerWindow>,
    drums: Weak<DrumsWindow>,
    plugins: Weak<PluginsWindow>,
    plugin_editor: Weak<PluginEditorWindow>,
    tone3000: Weak<Tone3000Window>,
    chain_editor: Rc<RefCell<Option<ChainEditorWindow>>>,
    plugin_info: Rc<RefCell<Option<PluginInfoWindow>>>,
}

impl SchemeWindows {
    fn of(windows: &DesktopWindows) -> Self {
        Self {
            app: windows.window.as_weak(),
            project_settings: windows.project_settings_window.as_weak(),
            chain_insert: windows.chain_insert_window.as_weak(),
            chain_port: windows.chain_port_window.as_weak(),
            tuner: windows.tuner_window.as_weak(),
            spectrum: windows.spectrum_window.as_weak(),
            metronome: windows.metronome_window.as_weak(),
            mixer: windows.mixer_window.as_weak(),
            player: windows.player_window.as_weak(),
            drums: windows.drums_window.as_weak(),
            plugins: windows.plugins_window.as_weak(),
            plugin_editor: windows.plugin_editor_window.as_weak(),
            tone3000: windows.tone3000_window.as_weak(),
            chain_editor: windows.chain_editor_window.clone(),
            plugin_info: windows.plugin_info_window.clone(),
        }
    }

    fn paint(&self) {
        if let Some(w) = self.app.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.project_settings.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.chain_insert.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.chain_port.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.tuner.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.spectrum.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.metronome.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.mixer.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.player.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.drums.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.plugins.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.plugin_editor.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.tone3000.upgrade() {
            apply(&w);
        }
        if let Some(w) = self.chain_editor.borrow().as_ref() {
            apply(w);
        }
        if let Some(w) = self.plugin_info.borrow().as_ref() {
            apply(w);
        }
    }
}

pub(crate) fn wire(
    windows: &DesktopWindows,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    app_config: Rc<RefCell<AppConfig>>,
) {
    let all = SchemeWindows::of(windows);
    crate::settings::appearance_wiring::wire(
        &windows.window,
        &windows.project_settings_window,
        project_session,
        app_config,
        move |appearance: Appearance| {
            remember(appearance);
            all.paint();
            crate::settings::appearance_followers::repaint_all(appearance);
        },
    );
}
