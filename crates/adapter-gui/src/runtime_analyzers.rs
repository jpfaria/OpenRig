//! Responsibility: owns the analyzer sessions' lifecycle.
//! #127: the ANALYZERS' lifecycle, behind the seam — the tuner (#544) and the
//! spectrum (#546).
//!
//! Powering an analyzer on is what makes it SUBSCRIBE and start reading: it
//! opens one tap per enabled chain's input (invariant #4 — by stream identity,
//! never a group), runs YIN / the FFT next to the audio on its own 33 ms tick,
//! and publishes the result through `LiveSource::tuner` / `LiveSource::spectrum`
//! — the same reading `openrig://tuner` and `openrig://spectrum` answer with.
//!
//! Until #127 that lifecycle lived in the GUI's POWER callback, so
//! `SelectionCommand::SetTunerEnabled` dispatched from `adapter-midi`'s
//! `toggle_tuner` footswitch slot (or from an MCP client) flipped the mirror in
//! `SelectionState`, reported its event and started nothing. This module is the
//! body of `RuntimeControl::set_tuner_running` / `set_spectrum_running`: the
//! command handler applies it, so every transport powers the same analyzer.
//!
//! **The window only renders.** The row model belongs to the session, and a
//! rebuild (the user enabled a chain, re-bound an input) produces a NEW one, so
//! the frontend installs a sink here once ([`AnalyzerSessions::on_tuner_rows`])
//! and this module hands it whatever model is current. Nothing in here knows a
//! window exists.
//!
//! **Never starts audio (#808).** An analyzer reads what is already sounding;
//! asking to look at a stopped rig is not asking to hear anything. With nothing
//! hosted it subscribes to nothing and publishes no rows, and the tick rebuilds
//! the session by itself once a runtime appears.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use application::audio_taps::{AudioTaps, NoAudioTaps};
use slint::{ModelRc, Timer, TimerMode};

use crate::spectrum_session::SpectrumSession;
use crate::state::ProjectSession;
use crate::tuner_session::TunerSession;
use crate::{SpectrumRow, TunerRow};

/// Detection cadence. ~30 Hz is plenty for both analyzers and is what the two
/// wiring modules used before the lifecycle moved here.
const TICK_INTERVAL: Duration = Duration::from_millis(33);

/// Where a frontend wants the current row model pushed. `None` ⇒ the analyzer
/// is off (or has nothing to show) and the view should clear.
type RowSink<R> = Box<dyn Fn(Option<ModelRc<R>>)>;

/// One analyzer's state: the session that owns the taps and the rows, whether
/// it is powered, its tick, and where to render.
struct Half<S, R: 'static> {
    session: Rc<RefCell<Option<S>>>,
    running: Cell<bool>,
    timer: Timer,
    render: RefCell<Option<RowSink<R>>>,
}

impl<S, R: 'static> Half<S, R> {
    fn new() -> Self {
        Self {
            session: Rc::new(RefCell::new(None)),
            running: Cell::new(false),
            timer: Timer::default(),
            render: RefCell::new(None),
        }
    }

    /// Hand the frontend the model it should be showing right now.
    fn publish(&self, rows: Option<ModelRc<R>>) {
        if let Some(sink) = self.render.borrow().as_ref() {
            sink(rows);
        }
    }
}

struct Inner {
    /// The open project, as the TICK sees it. Read only from the timer (never
    /// from a command handler, which may run inside a callback that already
    /// holds this cell borrowed — that is what the door's `session` argument is
    /// for).
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    taps: Rc<dyn AudioTaps>,
    tuner: Half<TunerSession, TunerRow>,
    spectrum: Half<SpectrumSession, SpectrumRow>,
}

/// The analyzers, as a cheap shared handle: the frontend keeps one, the
/// `RuntimeControl` impl holds another, and both address the same sessions.
#[derive(Clone)]
pub(crate) struct AnalyzerSessions {
    inner: Rc<Inner>,
}

impl AnalyzerSessions {
    pub(crate) fn new(
        project_session: &Rc<RefCell<Option<ProjectSession>>>,
        taps: &Rc<dyn AudioTaps>,
    ) -> Self {
        Self {
            inner: Rc::new(Inner {
                project_session: Rc::clone(project_session),
                taps: Rc::clone(taps),
                tuner: Half::new(),
                spectrum: Half::new(),
            }),
        }
    }

    /// The analyzers of a frontend that hosts no audio: powering one on
    /// subscribes to nothing and publishes no rows. What a `RuntimeControl`
    /// assembled outside the app (a test, a transport with no window) gets.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn detached() -> Self {
        Self::new(
            &Rc::new(RefCell::new(None)),
            &(Rc::new(NoAudioTaps) as Rc<dyn AudioTaps>),
        )
    }

    /// The cell `GuiLiveSource` reads the tuner's rows out of — the same
    /// session this module builds, so the window and `openrig://tuner` can
    /// never disagree.
    pub(crate) fn tuner_cell(&self) -> &Rc<RefCell<Option<TunerSession>>> {
        &self.inner.tuner.session
    }

    pub(crate) fn spectrum_cell(&self) -> &Rc<RefCell<Option<SpectrumSession>>> {
        &self.inner.spectrum.session
    }

    /// Install where the tuner's row model is rendered. Called once, by the
    /// wiring that owns the windows.
    pub(crate) fn on_tuner_rows(&self, sink: impl Fn(Option<ModelRc<TunerRow>>) + 'static) {
        *self.inner.tuner.render.borrow_mut() = Some(Box::new(sink));
    }

    pub(crate) fn on_spectrum_rows(&self, sink: impl Fn(Option<ModelRc<SpectrumRow>>) + 'static) {
        *self.inner.spectrum.render.borrow_mut() = Some(Box::new(sink));
    }

    /// The body of `RuntimeControl::set_tuner_running`.
    ///
    /// `session` is passed in rather than read from the cell above because this
    /// runs inside a command dispatch, which may be inside a GUI callback that
    /// is already holding it borrowed (the reason `SessionHandle` exists).
    /// `None` ⇒ no project is open, so there is nothing to subscribe to yet;
    /// the analyzer still powers on and its tick picks the project up.
    pub(crate) fn set_tuner_running(&self, session: Option<&ProjectSession>, running: bool) {
        let inner = &self.inner;
        inner.tuner.running.set(running);
        if !running {
            inner.tuner.timer.stop();
            *inner.tuner.session.borrow_mut() = None;
            inner.taps.prune_dead_taps();
            inner.tuner.publish(None);
            return;
        }
        let built = build_tuner(inner, session);
        inner
            .tuner
            .publish(built.as_ref().map(TunerSession::rows_model_rc));
        *inner.tuner.session.borrow_mut() = built;
        let weak = Rc::downgrade(inner);
        inner
            .tuner
            .timer
            .start(TimerMode::Repeated, TICK_INTERVAL, move || {
                if let Some(inner) = weak.upgrade() {
                    tick_tuner(&inner);
                }
            });
    }

    /// The body of `RuntimeControl::set_spectrum_running`. Same shape as the
    /// tuner's; the spectrum has no mute half.
    pub(crate) fn set_spectrum_running(&self, session: Option<&ProjectSession>, running: bool) {
        let inner = &self.inner;
        inner.spectrum.running.set(running);
        if !running {
            inner.spectrum.timer.stop();
            *inner.spectrum.session.borrow_mut() = None;
            inner.taps.prune_dead_taps();
            inner.spectrum.publish(None);
            return;
        }
        let built = build_spectrum(inner, session);
        inner
            .spectrum
            .publish(built.as_ref().map(SpectrumSession::rows_model_rc));
        *inner.spectrum.session.borrow_mut() = built;
        let weak = Rc::downgrade(inner);
        inner
            .spectrum
            .timer
            .start(TimerMode::Repeated, TICK_INTERVAL, move || {
                if let Some(inner) = weak.upgrade() {
                    tick_spectrum(&inner);
                }
            });
    }

    /// One detection tick, as the timer runs it. Exists so the analyzers can
    /// be driven headless, with no Slint event loop — the app itself always
    /// ticks from the timer these doors start.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn tick_tuner(&self) {
        tick_tuner(&self.inner);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn tick_spectrum(&self) {
        tick_spectrum(&self.inner);
    }
}

// ── the tuner ───────────────────────────────────────────────────────────

/// Subscribe every enabled chain's input channels. `None` ⇒ nothing is hosted:
/// a tuner over a stopped rig would show a list of strings that can never move.
fn build_tuner(inner: &Inner, session: Option<&ProjectSession>) -> Option<TunerSession> {
    let session = session?;
    if !inner.taps.is_hosted() {
        return None;
    }
    Some(TunerSession::build(
        &session.project.borrow(),
        inner.taps.as_ref(),
        &session.io_bindings.borrow(),
    ))
}

fn tick_tuner(inner: &Inner) {
    if !inner.tuner.running.get() {
        return;
    }
    if let Some(session) = inner.tuner.session.borrow_mut().as_mut() {
        session.tick();
    }
    let needs_rebuild = {
        let pj = inner.project_session.borrow();
        let session = inner.tuner.session.borrow();
        match (pj.as_ref(), session.as_ref()) {
            (Some(s), Some(sess)) => {
                sess.needs_rebuild(&s.project.borrow(), &s.io_bindings.borrow())
            }
            // The rig came up (or the project opened) while the tuner was on:
            // build the session the power-on could not.
            (Some(_), None) => true,
            _ => false,
        }
    };
    if !needs_rebuild {
        return;
    }
    let pj = inner.project_session.borrow();
    let Some(rebuilt) = build_tuner(inner, pj.as_ref()) else {
        return;
    };
    inner.tuner.publish(Some(rebuilt.rows_model_rc()));
    *inner.tuner.session.borrow_mut() = Some(rebuilt);
    inner.taps.prune_dead_taps();
}

// ── the spectrum ────────────────────────────────────────────────────────

fn build_spectrum(inner: &Inner, session: Option<&ProjectSession>) -> Option<SpectrumSession> {
    let session = session?;
    if !inner.taps.is_hosted() {
        return None;
    }
    Some(SpectrumSession::build(
        &session.project.borrow(),
        inner.taps.as_ref(),
        &session.io_bindings.borrow(),
    ))
}

fn tick_spectrum(inner: &Inner) {
    if !inner.spectrum.running.get() {
        return;
    }
    if let Some(session) = inner.spectrum.session.borrow_mut().as_mut() {
        session.tick();
    }
    let needs_rebuild = {
        let pj = inner.project_session.borrow();
        let session = inner.spectrum.session.borrow();
        match (pj.as_ref(), session.as_ref()) {
            (Some(s), Some(sess)) => {
                sess.needs_rebuild(&s.project.borrow(), &s.io_bindings.borrow())
            }
            (Some(_), None) => true,
            _ => false,
        }
    };
    if !needs_rebuild {
        return;
    }
    let rebuilt = {
        let pj = inner.project_session.borrow();
        build_spectrum(inner, pj.as_ref())
    };
    match rebuilt {
        Some(rebuilt) => {
            inner.spectrum.publish(Some(rebuilt.rows_model_rc()));
            *inner.spectrum.session.borrow_mut() = Some(rebuilt);
            inner.taps.prune_dead_taps();
        }
        None => {
            // No runtime (last chain disabled, runtime torn down). Drop the
            // stale session and clear the bars so the window does not freeze on
            // the last live frame.
            if let Some(session) = inner.spectrum.session.borrow_mut().as_mut() {
                session.freeze_to_zero();
            }
            *inner.spectrum.session.borrow_mut() = None;
            inner.spectrum.publish(None);
        }
    }
}

#[cfg(test)]
#[path = "runtime_analyzers_tests.rs"]
mod tests;
