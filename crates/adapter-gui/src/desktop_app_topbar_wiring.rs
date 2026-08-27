//! Responsibility: wires the top-bar features to the windows they open.
//!
//! Tuner, spectrum analyzer, metronome and the per-chain latency probe. Each
//! is powered through the analyzer sessions / live sources — the windows only
//! render, so a MIDI footswitch or an MCP client starts the very same feature
//! the button does (#127).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Timer, VecModel};

use crate::latency_probe;
use crate::state::ProjectSession;
use crate::{AppWindow, MetronomeWindow, SpectrumWindow, TunerWindow};

pub(crate) struct TopBarWindows<'a> {
    pub window: &'a AppWindow,
    pub tuner_window: &'a TunerWindow,
    pub spectrum_window: &'a SpectrumWindow,
    pub metronome_window: &'a MetronomeWindow,
}

pub(crate) fn wire(
    windows: TopBarWindows<'_>,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    project_chains: &Rc<VecModel<crate::ProjectChainItem>>,
    analyzers: &crate::runtime_analyzers::AnalyzerSessions,
    chain_rate: Rc<dyn application::live_source::LiveSource>,
    metronome_live: &Rc<dyn application::live_source::LiveSource>,
    metronome_timer: &Rc<Timer>,
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
}
