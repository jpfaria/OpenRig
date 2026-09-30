//! Responsibility: feeds one compact chain view the mixer strips of its own chain.
//! #1007 — the compact view shows, inline, only the inputs and outputs its
//! chain plays through. The strips are the global mixer's: gestures dispatch
//! the same `Command::Mixer` as the Mixer window, and a poll redraws when the
//! dispatcher's strips or the chain's bindings change (Mixer window, MCP,
//! MIDI), so both surfaces always agree.

use std::cell::RefCell;
use std::rc::Rc;

use application::chain_mixer_strips::chain_mixer_strip_ids;
use application::command::{Command, MixerCommand};
use application::mixer_view::MixerStripView;
use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::mixer_rows::mixer_rows_of;
use crate::mixer_rows_sync::set_mixer_rows;
use crate::mixer_strip_intents::wire_strip_intents;
use crate::state::ProjectSession;
use crate::{CompactChainViewWindow, MixerBridge};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// What is on screen: every strip (a solo elsewhere dims ours) + the chain's ids.
type Drawn = (Vec<MixerStripView>, Vec<String>);

#[derive(Clone)]
struct CompactMixerCtx {
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    window: slint::Weak<CompactChainViewWindow>,
    chain_index: usize,
    rendered: Rc<RefCell<Option<Drawn>>>,
}

impl CompactMixerCtx {
    fn current(&self) -> Drawn {
        let borrowed = self.project_session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return (Vec::new(), Vec::new());
        };
        let ids = session
            .project
            .borrow()
            .chains
            .get(self.chain_index)
            .map(|chain| chain_mixer_strip_ids(chain, &session.io_bindings.borrow()))
            .unwrap_or_default();
        (session.dispatcher.mixer_strips(), ids)
    }

    fn render(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let drawn = self.current();
        let (inputs, outputs) = mixer_rows_of(&drawn.0, &drawn.1);
        set_mixer_rows(&MixerBridge::get(&w), inputs, outputs);
        *self.rendered.borrow_mut() = Some(drawn);
    }

    fn dispatch(&self, command: MixerCommand) {
        {
            let borrowed = self.project_session.borrow();
            let Some(session) = borrowed.as_ref() else {
                return;
            };
            if let Err(e) = session.dispatcher.dispatch(Command::Mixer(command)) {
                log::warn!("[compact-mixer] dispatch failed: {e}");
                return;
            }
        }
        self.render();
    }
}

/// Draw chain `chain_index`'s strips in `compact_win` and keep them live.
pub(crate) fn wire(
    compact_win: &CompactChainViewWindow,
    chain_index: usize,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) {
    let ctx = CompactMixerCtx {
        project_session: project_session.clone(),
        window: compact_win.as_weak(),
        chain_index,
        rendered: Rc::new(RefCell::new(None)),
    };
    let c = ctx.clone();
    wire_strip_intents(
        &MixerBridge::get(compact_win),
        Rc::new(move |command| c.dispatch(command)),
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
