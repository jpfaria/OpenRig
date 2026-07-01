//! #717 — the DI fone in the DETACHED compact chain window must open the DI
//! panel. The compact view runs in its own `CompactChainViewWindow`; the fone
//! (`ChainDiLoopButton`) sets `DiPanel.open = true`, but the overlay that
//! renders the panel (`if DiPanel.open : DiLoopPanel {…}`) lived ONLY in the
//! main `app-window.slint`, never in the compact window — so in compact view
//! the fone did nothing ("botão não faz nada"). This drives a real pointer
//! click at the fone and asserts the panel appears in the SAME window.

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
