//! Responsibility: registers the TONE3000 window's callbacks.
//!
//! Everything the callbacks do lives in `Tone3000Ctx` (`tone3000_ctx.rs`),
//! which is tested; this file only hands it the windows, the intents and the
//! tick that collects searches and installs finishing off-thread (#879).

use std::rc::Rc;

use slint::{ComponentHandle, Global, Timer, TimerMode};

use crate::helpers::show_child_window;
use crate::tone3000_ctx::{ForwardEvents, Tone3000Ctx};
use crate::tone3000_intents::wire_tone3000_intents;
use crate::tone3000_session::SessionCell;
use crate::{AppWindow, Tone3000Bridge, Tone3000Window};

/// A download step takes seconds; a quarter second reads as live.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// Wire the top-bar button, the window's controls and the tick. Call once.
pub(crate) fn wire_tone3000(
    window: &AppWindow,
    tone3000_window: &Tone3000Window,
    project_session: &SessionCell,
    forward: ForwardEvents,
) {
    let ctx = Tone3000Ctx::new(project_session.clone(), tone3000_window.as_weak(), forward);
    let (d, p, a) = (ctx.clone(), ctx.clone(), ctx.clone());
    wire_tone3000_intents(
        tone3000_window,
        Rc::new(move |command| d.dispatch(command)),
        Rc::new(move |id, arch| p.pick(id, arch)),
        Rc::new(move |id| a.arch_of(id)),
    );
    let c = ctx.clone();
    let main = window.as_weak();
    Tone3000Bridge::get(window).on_open_tone3000_window(move || {
        let (Some(main_w), Some(tw)) = (main.upgrade(), c.window.upgrade()) else {
            return;
        };
        c.render();
        show_child_window(main_w.window(), tw.window());
    });
    // The closure owns its timer, so the tick lives as long as the app.
    let timer = Rc::new(Timer::default());
    let owned = timer.clone();
    timer.start(TimerMode::Repeated, POLL_INTERVAL, move || {
        let _ = &owned;
        ctx.tick();
    });
}
