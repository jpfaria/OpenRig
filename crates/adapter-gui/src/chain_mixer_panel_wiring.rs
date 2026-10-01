//! Responsibility: keeps the chain list's mixer overlay live.
//! #1007 — a chain card's mixer button opens that chain's mixer over the
//! list (`ChainMixerPanel.opened`). Its global halves are the chain's own
//! endpoints, drawn on `ChainMixerPanel`; their gestures reach the main
//! window's `MixerBridge`, which the Mixer window wiring already dispatches.
//! The chain's own faders sit on the main window's `ChainMixerBridge` and
//! dispatch the chain mixer commands (MASTER goes through the chain volume
//! path). While the overlay is open a poll redraws on any change (Mixer
//! window, compact view, MCP, MIDI).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use application::command::Command;
use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::chain_fader_intent::ChainFaderIntent;
use crate::chain_mixer_intents::wire_chain_mixer_intents;
use crate::chain_mixer_rows_sync::set_chain_mixer_rows;
use crate::chain_mixer_source::{chain_id_at, chain_mixer_state, chain_mixer_view, Drawn};
use crate::mixer_rows_sync::updated_in_place;
use crate::state::ProjectSession;
use crate::{AppWindow, ChainMixerBridge, ChainMixerPanel};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

#[derive(Clone)]
struct PanelCtx {
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    window: slint::Weak<AppWindow>,
    chain_index: Rc<Cell<usize>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    rendered: Rc<RefCell<Option<Drawn>>>,
    timer: Rc<Timer>,
}

impl PanelCtx {
    fn current(&self) -> Drawn {
        chain_mixer_state(&self.project_session, self.chain_index.get())
    }

    fn render(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let drawn = self.current();
        let view = chain_mixer_view(&self.project_session, &drawn);
        if let Some(rows) = view.chain {
            set_chain_mixer_rows(&ChainMixerBridge::get(&w), rows);
        }
        let panel = ChainMixerPanel::get(&w);
        if let Some(model) = updated_in_place(panel.get_global_inputs(), view.inputs) {
            panel.set_global_inputs(model);
        }
        if let Some(model) = updated_in_place(panel.get_global_outputs(), view.outputs) {
            panel.set_global_outputs(model);
        }
        *self.rendered.borrow_mut() = Some(drawn);
    }

    fn dispatch(&self, command: Command) -> bool {
        let borrowed = self.project_session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return false;
        };
        if let Err(e) = session.dispatcher.dispatch(command) {
            log::warn!("[chain-mixer] dispatch failed: {e}");
            return false;
        }
        true
    }

    fn chain(&self, intent: ChainFaderIntent) {
        match intent {
            ChainFaderIntent::Dispatch(command) => {
                if !self.dispatch(command) {
                    return;
                }
                self.mark_dirty();
            }
            ChainFaderIntent::MasterVolume(percent) => {
                if let Some(w) = self.window.upgrade() {
                    w.invoke_chain_volume_changed(self.chain_index.get() as i32, percent);
                }
            }
        }
        self.render();
    }

    fn mark_dirty(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let borrowed = self.project_session.borrow();
        if let Some(session) = borrowed.as_ref() {
            crate::project_dirty::sync_project_dirty(
                &w,
                session,
                &self.saved_project_snapshot,
                &self.project_dirty,
            );
        }
    }

    fn open(&self, chain_index: i32) {
        let Ok(index) = usize::try_from(chain_index) else {
            return;
        };
        self.chain_index.set(index);
        self.render();
        self.start_poll();
    }

    fn start_poll(&self) {
        let ctx = self.clone();
        self.timer
            .start(TimerMode::Repeated, POLL_INTERVAL, move || ctx.poll());
    }

    fn poll(&self) {
        let Some(w) = self.window.upgrade() else {
            self.timer.stop();
            return;
        };
        // Closed: nothing left on screen to keep live.
        if !ChainMixerPanel::get(&w).get_open() {
            self.timer.stop();
            return;
        }
        let current = self.current();
        if self.rendered.borrow().as_ref() != Some(&current) {
            self.render();
        }
    }
}

pub(crate) fn wire(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
) {
    let ctx = PanelCtx {
        project_session,
        window: window.as_weak(),
        chain_index: Rc::new(Cell::new(0)),
        saved_project_snapshot,
        project_dirty,
        rendered: Rc::new(RefCell::new(None)),
        timer: Rc::new(Timer::default()),
    };
    let (c, a) = (ctx.clone(), ctx.clone());
    wire_chain_mixer_intents(
        &ChainMixerBridge::get(window),
        Rc::new(move || chain_id_at(&c.project_session, c.chain_index.get())),
        Rc::new(move |intent| a.chain(intent)),
    );
    ChainMixerPanel::get(window).on_opened(move |chain_index| ctx.open(chain_index));
}

#[cfg(test)]
#[path = "chain_mixer_panel_wiring_tests.rs"]
mod tests;
