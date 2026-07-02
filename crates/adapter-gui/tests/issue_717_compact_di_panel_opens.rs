//! #717 — the DI fone in the DETACHED compact chain window must open the DI
//! panel. The compact view runs in its own `CompactChainViewWindow`; the fone
//! (`ChainDiLoopButton`) sets `DiPanel.open = true`, but the overlay that
//! renders the panel (`if DiPanel.open : DiLoopPanel {…}`) lived ONLY in the
//! main `app-window.slint`, never in the compact window — so in compact view
//! the fone did nothing ("botão não faz nada"). This drives a real pointer
//! click at the fone and asserts the panel appears in the SAME window.

use std::cell::Cell;
use std::rc::Rc;

use adapter_gui::CompactChainViewWindow;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, SharedString, VecModel};

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

fn count_id(w: &impl ComponentHandle, id: &str) -> usize {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, id).count()
}

#[test]
fn compact_window_fone_opens_the_di_panel() {
    i_slint_backend_testing::init_no_event_loop();

    let w = CompactChainViewWindow::new().unwrap();
    w.set_chain_index(0);
    w.set_di_loop_sources(ModelRc::new(VecModel::from(vec![
        SharedString::from("clean-electric-guitar-loop"),
        SharedString::from("Choose file…"),
    ])));
    w.show().unwrap();

    // The fone is present; the panel is NOT open yet.
    assert!(
        count_id(&w, "ChainDiLoopButton::ta") >= 1,
        "the compact window must show the DI fone"
    );
    assert_eq!(
        count_id(&w, "DiLoopPanel::sel-ta"),
        0,
        "the DI panel must be closed until the fone is clicked"
    );

    // Click the fone.
    assert!(
        click_id(&w, "ChainDiLoopButton::ta", 0),
        "the DI fone must be hittable"
    );

    // The panel must now be rendered IN THIS WINDOW.
    assert!(
        count_id(&w, "DiLoopPanel::sel-ta") >= 1,
        "#717: clicking the fone in the detached compact window must open the DI \
         panel — today the overlay is missing from CompactChainViewWindow so the \
         button does nothing"
    );
}

/// #717 — with a source selected, the compact panel must reflect the selection
/// and expose the play/stop control. The window must plumb
/// `di-loop-selected-index` through to the fone/panel; without it the panel
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
        click_id(&w, "ChainDiLoopButton::ta", 0),
        "the DI fone must be hittable"
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

/// #717 — the dedicated DI-stream graph appears in the compact window only while
/// the DI is playing, and disappears on stop.
#[test]
fn compact_di_graph_shows_only_while_playing() {
    i_slint_backend_testing::init_no_event_loop();

    let w = CompactChainViewWindow::new().unwrap();
    w.set_chain_index(0);
    w.set_di_graph_output_label(SharedString::from("Scarlett 1-2"));
    w.set_di_loop_playing(false);
    w.show().unwrap();

    assert_eq!(
        count_id(&w, "CompactChainViewPage::di-graph"),
        0,
        "#717: the DI graph must be hidden when the DI is not playing"
    );

    w.set_di_loop_playing(true);
    assert!(
        count_id(&w, "CompactChainViewPage::di-graph") >= 1,
        "#717: the DI graph must appear while the DI plays"
    );

    w.set_di_loop_playing(false);
    assert_eq!(
        count_id(&w, "CompactChainViewPage::di-graph"),
        0,
        "#717: the DI graph must disappear on stop"
    );
}
