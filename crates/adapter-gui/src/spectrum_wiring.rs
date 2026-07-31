//! SpectrumWindow wiring — all callbacks for the top-bar Spectrum feature.
//! Mirrors `tuner_wiring.rs` (open / close / power) minus the mute path — the
//! spectrum view is a passive display.
//!
//! `wire_spectrum` is the single entry point. `lib.rs` calls it once
//! during window setup and never touches spectrum logic again.
//!
//! #127: like the tuner, this module RENDERS. `SelectionCommand::
//! SetSpectrumEnabled` powers the analyzer through `RuntimeControl`
//! (`runtime_analyzers`), so the `toggle_spectrum` footswitch and an MCP
//! client start the same FFT this button does; the bars' model arrives through
//! the sink installed below.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, SelectionCommand};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::helpers::{show_child_window, use_inline_block_editor};
use crate::runtime_analyzers::AnalyzerSessions;
use crate::spectrum_close::spectrum_close_commands;
use crate::state::ProjectSession;
use crate::{AppWindow, SpectrumRow, SpectrumWindow};

/// Wire every Spectrum callback (open / close / power) onto the supplied
/// windows. Idempotent in spirit but should only be called once per
/// `AppWindow + SpectrumWindow` pair during application startup.
pub fn wire_spectrum(
    window: &AppWindow,
    spectrum_window: &SpectrumWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    analyzers: &AnalyzerSessions,
) {
    install_row_sink(window, spectrum_window, analyzers);
    wire_open(window, spectrum_window);
    wire_close_inline(window, project_session);
    wire_close_windowed(spectrum_window, project_session);
    wire_power(window, spectrum_window, project_session);
}

/// Where the analyzer's bars are rendered — see `tuner_wiring::install_row_sink`
/// for why the model has to be re-bound on every rebuild.
fn install_row_sink(
    window: &AppWindow,
    spectrum_window: &SpectrumWindow,
    analyzers: &AnalyzerSessions,
) {
    let main_window_weak = window.as_weak();
    let spectrum_window_weak = spectrum_window.as_weak();
    analyzers.on_spectrum_rows(move |rows| {
        let rows = rows.unwrap_or_else(empty_rows_model);
        if let Some(sw) = spectrum_window_weak.upgrade() {
            sw.set_spectrum_rows(rows.clone());
        }
        if let Some(mw) = main_window_weak.upgrade() {
            mw.set_spectrum_rows(rows);
        }
    });
}

fn wire_open(window: &AppWindow, spectrum_window: &SpectrumWindow) {
    let spectrum_window_weak = spectrum_window.as_weak();
    let main_window_weak = window.as_weak();
    window.on_open_spectrum_window(move || {
        let Some(sw) = spectrum_window_weak.upgrade() else {
            return;
        };
        let Some(main_w) = main_window_weak.upgrade() else {
            return;
        };
        let inline = use_inline_block_editor(&main_w);

        // Open the spectrum in the powered-off resting state: no session,
        // no polling timer, no rows. The user has to press POWER to start
        // the analyzer (see `wire_power`).
        let empty = empty_rows_model();
        if inline {
            main_w.set_spectrum_rows(empty);
            main_w.set_spectrum_enabled(false);
            main_w.set_show_spectrum(true);
        } else {
            sw.set_spectrum_rows(empty);
            sw.set_spectrum_enabled(false);
            // Same window-opening pattern as the Block Editor: position
            // the child window relative to the main window so the user
            // sees it appear next to where they clicked instead of at
            // an OS-default location.
            show_child_window(main_w.window(), sw.window());
        }
    });
}

fn wire_close_inline(window: &AppWindow, project_session: &Rc<RefCell<Option<ProjectSession>>>) {
    let project_session = project_session.clone();
    let main_window_weak = window.as_weak();
    window.on_close_spectrum(move || {
        dispatch_close_commands(&project_session);
        if let Some(mw) = main_window_weak.upgrade() {
            mw.set_show_spectrum(false);
            // #546: keep the Slint power state in sync with the backend
            // going off. Without this, the toggle could stay lit on the
            // next inline render until wire_open's reset runs.
            mw.set_spectrum_enabled(false);
        }
    });
}

fn wire_close_windowed(
    spectrum_window: &SpectrumWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    // Mirrors `tuner_wiring::wire_close_windowed` (#544). The windowed
    // SpectrumWindow renders with `show-close-button: false`, so
    // `on_close_spectrum_window` never fires today; the only way to
    // close that window is the OS chrome (X / Cmd-W), which Slint
    // routes through `Window::on_close_requested`. Wire BOTH so neither
    // path leaves the FFT analyzer + stream taps alive (#546).
    {
        let project_session = project_session.clone();
        let spectrum_window_weak = spectrum_window.as_weak();
        spectrum_window.on_close_spectrum_window(move || {
            close_spectrum_windowed_impl(&project_session, &spectrum_window_weak);
            if let Some(sw) = spectrum_window_weak.upgrade() {
                let _ = sw.hide();
            }
        });
    }
    {
        let project_session = project_session.clone();
        let spectrum_window_weak = spectrum_window.as_weak();
        spectrum_window.window().on_close_requested(move || {
            close_spectrum_windowed_impl(&project_session, &spectrum_window_weak);
            slint::CloseRequestResponse::HideWindow
        });
    }
}

fn close_spectrum_windowed_impl(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    spectrum_window_weak: &slint::Weak<SpectrumWindow>,
) {
    dispatch_close_commands(project_session);
    if let Some(sw) = spectrum_window_weak.upgrade() {
        sw.set_spectrum_enabled(false);
    }
}

fn dispatch_close_commands(project_session: &Rc<RefCell<Option<ProjectSession>>>) {
    // #546 + architectural law "every state change is a Command": close
    // routes through the shared dispatcher so MCP / MIDI / future gRPC
    // see Event::SpectrumEnabledChanged { false } instead of the adapter
    // mutating runtime state silently. #127: the same dispatch also stops the
    // analyzer and releases its taps.
    for cmd in spectrum_close_commands() {
        dispatch(project_session, cmd);
    }
}

fn wire_power(
    window: &AppWindow,
    spectrum_window: &SpectrumWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    let project_session = project_session.clone();
    let main_window_weak = window.as_weak();
    let spectrum_window_weak = spectrum_window.as_weak();

    let on_toggle_enabled = move |enabled: bool| {
        // #436 H / #127: POWER is business → a Command on the shared
        // dispatcher, which builds or tears down the FFT session through
        // `RuntimeControl::set_spectrum_running`. The bars arrive through the
        // sink, whoever asked for them.
        dispatch(
            &project_session,
            Command::Selection(SelectionCommand::SetSpectrumEnabled { enabled }),
        );
        // Always reflect the new enabled state on the UI even if no session
        // could be built, so the toggle never traps OFF.
        if let Some(sw) = spectrum_window_weak.upgrade() {
            sw.set_spectrum_enabled(enabled);
        }
        if let Some(mw) = main_window_weak.upgrade() {
            mw.set_spectrum_enabled(enabled);
        }
    };
    let cloned = on_toggle_enabled.clone();
    window.on_toggle_spectrum_enabled(cloned);
    spectrum_window.on_toggle_enabled(on_toggle_enabled);
}

// ── helpers ─────────────────────────────────────────────────────────────

fn empty_rows_model() -> ModelRc<SpectrumRow> {
    ModelRc::from(Rc::new(VecModel::from(Vec::<SpectrumRow>::new())))
}

/// Clone the dispatcher handle out FIRST: dispatching applies the runtime
/// effect, and that must not run while the session cell is still borrowed.
fn dispatch(project_session: &Rc<RefCell<Option<ProjectSession>>>, cmd: Command) {
    let dispatcher = project_session
        .borrow()
        .as_ref()
        .map(|s| s.dispatcher.clone());
    let Some(dispatcher) = dispatcher else {
        return;
    };
    if let Err(e) = dispatcher.dispatch(cmd) {
        log::warn!("[spectrum] command failed: {e}");
    }
}
