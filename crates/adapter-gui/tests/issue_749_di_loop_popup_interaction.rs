//! #749 — HEADLESS interaction proof for the DI loop play/stop control.
//!
//! Render proves layout only. The slint testing backend CANNOT actuate
//! PopupWindow content (a separate surface — verified: clicking a row or the
//! play button inside a popup fired nothing), which is exactly why the
//! popup-with-play-inside design was unverifiable and flaky. So the play/stop
//! lives on the header (main window) where a real pointer event DOES reach it,
//! and this test proves it: with a source selected the play button appears and
//! clicking it fires `di-loop-play`. The source picker mirrors the proven
//! `preset_select` popup (pick selects + closes) — its reliability rides on
//! that shared, in-app-proven pattern.

use adapter_gui::DiLoopHarness;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, SharedString, VecModel};
use std::cell::Cell;
use std::rc::Rc;

fn click_id(w: &impl ComponentHandle, id: &str, nth: usize) -> bool {
    let Some(el) = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).nth(nth) else {
        return false;
    };
    let pos = el.absolute_position();
    let sz = el.size();
    let c = LogicalPosition::new(pos.x + sz.width / 2.0, pos.y + sz.height / 2.0);
    let win = w.window();
    win.dispatch_event(WindowEvent::PointerMoved { position: c });
    win.dispatch_event(WindowEvent::PointerPressed {
        position: c,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerReleased {
        position: c,
        button: PointerEventButton::Left,
    });
    win.dispatch_event(WindowEvent::PointerExited);
    true
}

fn exists(w: &impl ComponentHandle, id: &str) -> bool {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id)
        .next()
        .is_some()
}

#[test]
fn di_loop_header_play_button_fires_when_a_source_is_selected() {
    i_slint_backend_testing::init_no_event_loop();

    let w = DiLoopHarness::new().unwrap();
    w.set_sources(ModelRc::new(VecModel::from(vec![
        SharedString::from("clean-electric-guitar-loop"),
        SharedString::from("fabiano-antunes-strato-clean"),
        SharedString::from("Choose file…"),
    ])));

    let played = Rc::new(Cell::new(false));
    let pl = played.clone();
    w.on_play(move || pl.set(true));
    let stopped = Rc::new(Cell::new(false));
    let st = stopped.clone();
    w.on_stop(move || st.set(true));

    // 1. With NO source selected, the play button is absent (nothing to play).
    w.set_selected_index(-1);
    w.set_playing(false);
    w.show().unwrap();
    assert!(
        exists(&w, "ChainDiLoopButton::ta"),
        "the fone (picker trigger) must always be present"
    );
    assert!(
        !exists(&w, "ChainDiLoopButton::play-ta"),
        "play must be hidden until a source is selected"
    );

    // 2. Once a source is selected, the play button appears and is hittable,
    //    firing di-loop-play.
    w.set_selected_index(1);
    assert!(
        click_id(&w, "ChainDiLoopButton::play-ta", 0),
        "play button must be hittable once a source is selected"
    );
    assert!(
        played.get(),
        "REGRESSION #749: clicking the header play button did not fire di-loop-play"
    );

    // 3. While playing, the same button stops.
    w.set_playing(true);
    assert!(click_id(&w, "ChainDiLoopButton::play-ta", 0), "stop must be hittable");
    assert!(
        stopped.get(),
        "REGRESSION #749: clicking the header button while playing did not fire di-loop-stop"
    );
}
