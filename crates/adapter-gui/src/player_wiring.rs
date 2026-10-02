//! Responsibility: wires the backing-track player window.
//!
//! The player shows in two places — the standalone `PlayerWindow` (windowed
//! desktop) and inline over the chains page (fullscreen / touch) — and both
//! host the same `PlayerPanel`, which reads the `PlayerBridge` global. So every
//! write and every callback goes through the bridge of BOTH surfaces.
//!
//! Closing the panel only hides it: the track keeps playing in its own stream
//! until STOP or PAUSE, like the metronome's click.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use application::live_source::LiveSource;
use application::player_state::PlayerSnapshot;
use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::helpers::{show_child_window, use_inline_block_editor};
use crate::metronome_view::MetronomeOutput;
use crate::player_controls_wiring::wire_controls;
use crate::player_render::{render_library, render_reading, render_snapshot};
use crate::state::ProjectSession;
use crate::{AppWindow, PlayerBridge, PlayerWindow};

/// How often the clock and the seek bar follow the track.
const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

/// Everything the player callbacks reach.
pub(crate) struct PlayerCtx {
    pub(crate) project_session: Rc<RefCell<Option<ProjectSession>>>,
    /// Where the track is, read from the player's own stream.
    pub(crate) live: Rc<dyn LiveSource>,
    pub(crate) timer: Rc<Timer>,
    pub(crate) window: slint::Weak<PlayerWindow>,
    pub(crate) main_window: slint::Weak<AppWindow>,
    /// The project's output endpoints as the select shows them.
    pub(crate) outputs: Rc<RefCell<Vec<MetronomeOutput>>>,
    /// The snapshot the panel currently shows, so the timer re-renders only
    /// when another transport changed it.
    pub(crate) rendered: Rc<RefCell<Option<PlayerSnapshot>>>,
    /// The loop start the first LOOP press marked, until the second closes it.
    pub(crate) loop_mark: Rc<Cell<Option<f64>>>,
}

impl PlayerCtx {
    pub(crate) fn clone_ctx(&self) -> Self {
        Self {
            project_session: self.project_session.clone(),
            live: self.live.clone(),
            timer: self.timer.clone(),
            window: self.window.clone(),
            main_window: self.main_window.clone(),
            outputs: self.outputs.clone(),
            rendered: self.rendered.clone(),
            loop_mark: self.loop_mark.clone(),
        }
    }

    /// Run `f` against the `PlayerBridge` of every live surface.
    pub(crate) fn for_each_bridge(&self, mut f: impl FnMut(&PlayerBridge)) {
        if let Some(w) = self.window.upgrade() {
            f(&PlayerBridge::get(&w));
        }
        if let Some(w) = self.main_window.upgrade() {
            f(&PlayerBridge::get(&w));
        }
    }

    /// The player state the dispatcher owns; `None` with no project open.
    pub(crate) fn snapshot(&self) -> Option<PlayerSnapshot> {
        self.project_session
            .borrow()
            .as_ref()
            .map(|session| session.dispatcher.player_snapshot())
    }
}

/// Wire the player's open, close and control callbacks on both surfaces.
pub fn wire_player(
    window: &AppWindow,
    player_window: &PlayerWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    live: &Rc<dyn LiveSource>,
) {
    let ctx = PlayerCtx {
        project_session: project_session.clone(),
        live: live.clone(),
        timer: Rc::new(Timer::default()),
        window: player_window.as_weak(),
        main_window: window.as_weak(),
        outputs: Rc::new(RefCell::new(Vec::new())),
        rendered: Rc::new(RefCell::new(None)),
        loop_mark: Rc::new(Cell::new(None)),
    };
    for bridge in [PlayerBridge::get(player_window), PlayerBridge::get(window)] {
        wire_open(&bridge, &ctx);
        let close_ctx = ctx.clone_ctx();
        bridge.on_close_player(move || close_player(&close_ctx));
        wire_controls(&bridge, &ctx);
    }
    let close_ctx = ctx.clone_ctx();
    player_window.window().on_close_requested(move || {
        close_player(&close_ctx);
        slint::CloseRequestResponse::HideWindow
    });
}

fn wire_open(bridge: &PlayerBridge, ctx: &PlayerCtx) {
    let ctx = ctx.clone_ctx();
    bridge.on_open_player_window(move || {
        let Some(main_w) = ctx.main_window.upgrade() else {
            return;
        };
        render_library(&ctx);
        render_snapshot(&ctx);
        render_reading(&ctx);
        start_timer(&ctx);
        if use_inline_block_editor(&main_w) {
            PlayerBridge::get(&main_w).set_show(true);
        } else if let Some(w) = ctx.window.upgrade() {
            show_child_window(main_w.window(), w.window());
        }
    });
}

/// Hide whichever surface shows the player. The track keeps playing.
fn close_player(ctx: &PlayerCtx) {
    if let Some(w) = ctx.main_window.upgrade() {
        PlayerBridge::get(&w).set_show(false);
    }
    if let Some(w) = ctx.window.upgrade() {
        let _ = w.hide();
    }
    ctx.timer.stop();
}

/// Follow the track's position while a surface is visible, and redraw the
/// controls when another transport changed the dispatcher's state.
fn start_timer(ctx: &PlayerCtx) {
    let ctx = ctx.clone_ctx();
    ctx.timer
        .clone()
        .start(TimerMode::Repeated, TICK_INTERVAL, move || {
            render_reading(&ctx);
            // The borrow ends on its own line: `render_snapshot` borrows
            // `rendered` mutably.
            let current = ctx.snapshot();
            let changed = ctx.rendered.borrow().as_ref() != current.as_ref();
            if changed {
                render_snapshot(&ctx);
            }
        });
}
