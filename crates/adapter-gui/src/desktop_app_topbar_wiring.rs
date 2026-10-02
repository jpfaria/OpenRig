//! Responsibility: wires the top-bar features to the windows they open.
//!
//! Tuner, spectrum analyzer, metronome, backing-track player, mixer and the per-chain latency probe. Each
//! is powered through the analyzer sessions / live sources — the windows only
//! render, so a MIDI footswitch or an MCP client starts the very same feature
//! the button does (#127).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Timer, VecModel};

use crate::latency_probe;
use crate::state::ProjectSession;
use crate::{AppWindow, MetronomeWindow, MixerWindow, PlayerWindow, SpectrumWindow, TunerWindow};

pub(crate) struct TopBarWindows<'a> {
    pub window: &'a AppWindow,
    pub tuner_window: &'a TunerWindow,
    pub spectrum_window: &'a SpectrumWindow,
    pub metronome_window: &'a MetronomeWindow,
    pub mixer_window: &'a MixerWindow,
    pub player_window: &'a PlayerWindow,
}

pub(crate) fn wire(
    windows: TopBarWindows<'_>,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_chains: &Rc<VecModel<crate::ProjectChainItem>>,
    analyzers: &crate::runtime_analyzers::AnalyzerSessions,
    chain_rate: Rc<dyn application::live_source::LiveSource>,
    metronome_live: &Rc<dyn application::live_source::LiveSource>,
    metronome_timer: &Rc<Timer>,
    player_live: &Rc<dyn application::live_source::LiveSource>,
    probe_windows: latency_probe::ProbeWindows,
) {
    latency_probe::install_handler(
        windows.window,
        project_session.clone(),
        project_chains.clone(),
        probe_windows,
        chain_rate,
    );
    crate::tuner_wiring::wire_tuner(
        windows.window,
        windows.tuner_window,
        project_session,
        analyzers,
    );
    crate::spectrum_wiring::wire_spectrum(
        windows.window,
        windows.spectrum_window,
        project_session,
        analyzers,
    );
    crate::metronome_wiring::wire_metronome(
        windows.window,
        windows.metronome_window,
        project_session,
        metronome_live,
        metronome_timer,
    );
    crate::player_wiring::wire_player(
        windows.window,
        windows.player_window,
        project_session,
        player_live,
    );
    crate::mixer_wiring::wire_mixer(windows.window, windows.mixer_window, project_session);
}
