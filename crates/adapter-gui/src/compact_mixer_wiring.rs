//! Responsibility: keeps one compact chain view's mixer tabs live.
//! #1007 — the compact view shows, per endpoint its chain plays through, the
//! global strip beside the chain's own fader, plus the chain's DI, LOOPER and
//! MASTER faders. Global gestures dispatch the same `Command::Mixer` as the
//! Mixer window; chain gestures dispatch the chain mixer commands (MASTER goes
//! through the chain volume path). A poll redraws when the dispatcher's strips,
//! the chain's bindings or the chain itself change (Mixer window, MCP, MIDI),
//! so every surface always agrees.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, MixerCommand};
use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::chain_fader_intent::ChainFaderIntent;
use crate::chain_mixer_intents::wire_chain_mixer_intents;
use crate::chain_mixer_rows_sync::set_chain_mixer_rows;
use crate::chain_mixer_source::{chain_id_at, chain_mixer_state, chain_mixer_view, Drawn};
use crate::mixer_rows_sync::set_mixer_rows;
use crate::mixer_strip_intents::wire_strip_intents;
use crate::state::ProjectSession;
use crate::{AppWindow, ChainMixerBridge, CompactChainViewWindow, MixerBridge};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// Where a chain fader edit marks the project unsaved.
#[derive(Clone)]
pub(crate) struct CompactMixerDirty {
    pub main_window: slint::Weak<AppWindow>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
}

#[derive(Clone)]
struct CompactMixerCtx {
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    window: slint::Weak<CompactChainViewWindow>,
    chain_index: usize,
    dirty: CompactMixerDirty,
    rendered: Rc<RefCell<Option<Drawn>>>,
}

impl CompactMixerCtx {
    fn current(&self) -> Drawn {
        chain_mixer_state(&self.project_session, self.chain_index)
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
        set_mixer_rows(&MixerBridge::get(&w), view.inputs, view.outputs);
        *self.rendered.borrow_mut() = Some(drawn);
    }

    fn chain_id(&self) -> Option<domain::ids::ChainId> {
        chain_id_at(&self.project_session, self.chain_index)
    }

    fn dispatch(&self, command: Command) -> bool {
        let borrowed = self.project_session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return false;
        };
        if let Err(e) = session.dispatcher.dispatch(command) {
            log::warn!("[compact-mixer] dispatch failed: {e}");
            return false;
        }
        true
    }

    fn global(&self, command: MixerCommand) {
        if self.dispatch(Command::Mixer(command)) {
            self.render();
        }
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
                    w.invoke_chain_volume_changed(self.chain_index as i32, percent);
                }
            }
        }
        self.render();
    }

    fn mark_dirty(&self) {
        let Some(main) = self.dirty.main_window.upgrade() else {
            return;
        };
        let borrowed = self.project_session.borrow();
        if let Some(session) = borrowed.as_ref() {
            crate::project_dirty::sync_project_dirty(
                &main,
                session,
                &self.dirty.saved_project_snapshot,
                &self.dirty.project_dirty,
            );
        }
    }
}

/// Draw chain `chain_index`'s mixer tabs in `compact_win` and keep them live.
pub(crate) fn wire(
    compact_win: &CompactChainViewWindow,
    chain_index: usize,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    dirty: CompactMixerDirty,
) {
    let ctx = CompactMixerCtx {
        project_session: project_session.clone(),
        window: compact_win.as_weak(),
        chain_index,
        dirty,
        rendered: Rc::new(RefCell::new(None)),
    };
    let c = ctx.clone();
    wire_strip_intents(
        &MixerBridge::get(compact_win),
        Rc::new(move |command| c.global(command)),
    );
    let (c, a) = (ctx.clone(), ctx.clone());
    wire_chain_mixer_intents(
        &ChainMixerBridge::get(compact_win),
        Rc::new(move || c.chain_id()),
        Rc::new(move |intent| a.chain(intent)),
    );
    ctx.render();
    start_poll(ctx);
}

fn start_poll(ctx: CompactMixerCtx) {
    let timer = Rc::new(Timer::default());
    let own = Rc::downgrade(&timer);
    timer.start(TimerMode::Repeated, POLL_INTERVAL, move || {
        // The window is gone: this poll has nothing left to feed.
        let Some(w) = ctx.window.upgrade() else {
            if let Some(t) = own.upgrade() {
                t.stop();
            }
            return;
        };
        if !w.window().is_visible() {
            return;
        }
        let current = ctx.current();
        if ctx.rendered.borrow().as_ref() != Some(&current) {
            ctx.render();
        }
    });
    // The timer lives as long as its window; it stops itself above.
    std::mem::forget(timer);
}
