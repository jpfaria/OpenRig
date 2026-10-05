//! #717 — the DI control in the DETACHED compact chain window must open the DI
//! panel IN that window ("botão não faz nada" when the panel lived only in the
//! main window). Since #1022 the control is the view's DI accordion section,
//! not a header fone: this drives a real pointer click at the section header
//! and asserts the panel appears in the SAME window.

use std::cell::Cell;
use std::rc::Rc;

use adapter_gui::CompactChainViewWindow;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, SharedString, VecModel};

fn click_id(w: &impl ComponentHandle, id: &str, nth: usize) -> bool {
    let Some(el) = i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).nth(nth)
    else {
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

const DI_SECTION: &str = "CompactChainSections::di-toggle";

fn count_id(w: &impl ComponentHandle, id: &str) -> usize {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).count()
}

#[test]
fn compact_window_di_section_opens_the_di_panel() {
    i_slint_backend_testing::init_no_event_loop();

    let w = CompactChainViewWindow::new().unwrap();
    w.set_chain_index(0);
    w.set_di_loop_sources(ModelRc::new(VecModel::from(vec![
        SharedString::from("clean-electric-guitar-loop"),
        SharedString::from("Choose file…"),
    ])));
    w.show().unwrap();

    // The DI section is present; the panel is NOT open yet.
    assert!(
        count_id(&w, DI_SECTION) >= 1,
        "the compact window must show the DI section"
    );
    assert_eq!(
        count_id(&w, "DiLoopPanel::sel-ta"),
        0,
        "the DI panel must be closed until its section opens"
    );

    // Open the section.
    assert!(
        click_id(&w, DI_SECTION, 0),
        "the DI section header must be hittable"
    );

    // The panel must now be rendered IN THIS WINDOW.
    assert!(
        count_id(&w, "DiLoopPanel::sel-ta") >= 1,
        "#717: opening the DI section in the detached compact window must show the \
         DI panel in that window"
    );
}

/// #717 — with a source selected, the compact panel must reflect the selection
/// and expose the play/stop control. The window must plumb
/// `di-loop-selected-index` through to the panel; without it the panel
/// opens at -1 (nothing selected), so it shows no source and hides play/stop —
/// "não mostra o que selecionei e não consigo dar stop".
#[test]
fn compact_panel_reflects_selection_and_can_stop() {
    i_slint_backend_testing::init_no_event_loop();

    let w = CompactChainViewWindow::new().unwrap();
    w.set_chain_index(0);
    w.set_di_loop_sources(ModelRc::new(VecModel::from(vec![
        SharedString::from("clean-electric-guitar-loop"),
        SharedString::from("Choose file…"),
    ])));
    w.set_di_loop_selected_index(0);
    w.set_di_loop_playing(true);

    let stopped = Rc::new(Cell::new(false));
    let s = stopped.clone();
    w.on_di_loop_stop(move || s.set(true));

    w.show().unwrap();

    assert!(
        click_id(&w, DI_SECTION, 0),
        "the DI section header must be hittable"
    );

    // A selected source must surface the play/stop button (it renders only when
    // the panel has a selection).
    assert!(
        count_id(&w, "DiLoopPanel::play-ta") >= 1,
        "#717: a selected source must show the play/stop button in the compact panel"
    );

    // Playing → the control stops.
    assert!(
        click_id(&w, "DiLoopPanel::play-ta", 0),
        "the play/stop control must be hittable"
    );
    assert!(
        stopped.get(),
        "#717: clicking the control while playing must fire di-loop-stop"
    );
}

/// #717 — the DI's own IN/OUT meter row appears among the meter rows only while
/// the DI is playing, and disappears on stop.
#[test]
fn compact_di_meter_row_shows_only_while_playing() {
    i_slint_backend_testing::init_no_event_loop();

    let w = CompactChainViewWindow::new().unwrap();
    w.set_chain_index(0);
    w.set_di_loop_playing(false);
    w.show().unwrap();
    // #1022: the meter rows sit in the Meters section (running chain only).
    w.set_chain_enabled(true);
    assert!(
        click_id(&w, "CompactChainSections::meters-toggle", 0),
        "the meters section header must be hittable"
    );

    assert_eq!(
        count_id(&w, "CompactStreamMeters::di-row"),
        0,
        "#717: the DI meter row must be hidden when the DI is not playing"
    );

    w.set_di_loop_playing(true);
    assert!(
        count_id(&w, "CompactStreamMeters::di-row") >= 1,
        "#717: the DI meter row must appear among the meters while the DI plays"
    );

    w.set_di_loop_playing(false);
    assert_eq!(
        count_id(&w, "CompactStreamMeters::di-row"),
        0,
        "#717: the DI meter row must disappear on stop"
    );
}
