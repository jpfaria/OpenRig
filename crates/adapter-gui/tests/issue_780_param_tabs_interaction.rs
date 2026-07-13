//! #780 — HEADLESS proof of the block editor's parameter TAB BAR: the tabs are
//! plain TouchAreas (not a PopupWindow), so the headless backend can dispatch a
//! REAL pointer event at a tab and confirm the tap lands + fires `select(i)`
//! with that tab's index. This is the interaction the flat param wall replaces
//! for VST3 plugins with many grouped parameters.

use adapter_gui::ParamTabBarHarness;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

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

#[test]
fn clicking_a_tab_fires_select_with_its_index() {
    i_slint_backend_testing::init_no_event_loop();

    let w = ParamTabBarHarness::new().unwrap();
    w.set_groups(ModelRc::new(VecModel::from(vec![
        SharedString::from("Tone"),
        SharedString::from("Voicing"),
        SharedString::from("Drive"),
    ])));
    w.set_active(0);

    let picked: Rc<RefCell<Option<i32>>> = Rc::new(RefCell::new(None));
    let p = picked.clone();
    w.on_select(move |i| *p.borrow_mut() = Some(i));

    w.show().unwrap();

    // The three tabs each expose a hittable TouchArea.
    assert_eq!(
        i_slint_backend_testing::ElementHandle::find_by_element_id(&w, "ParamTabBar::tab-ta")
            .count(),
        3,
        "one hittable tab per group"
    );

    // Click the 3rd tab ("Drive") → select fires with index 2.
    assert!(
        click_id(&w, "ParamTabBar::tab-ta", 2),
        "a tab must be hittable"
    );
    assert_eq!(
        *picked.borrow(),
        Some(2),
        "clicking the 3rd tab must fire select(2)"
    );
}
