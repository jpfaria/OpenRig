//! #787 — HEADLESS proof that the compact chain view's parameter TABS are
//! actually reachable: instantiate the real `CompactChainViewWindow`, feed it a
//! block with 3 parameter groups, dispatch a REAL pointer event at the 2nd tab
//! and confirm it fires `select-block-parameter-group` with that block and that
//! group. A render only proves the bar is drawn; this proves it can be clicked.

use adapter_gui::{CompactBlockItem, CompactChainViewWindow, CompactParamLine};
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

/// A compact block with 3 parameter groups and one line of (no) cells — enough
/// for the row to render its tab bar.
fn tabbed_block() -> CompactBlockItem {
    CompactBlockItem {
        chain_index: 0,
        block_index: 4,
        block_id: "amp-1".into(),
        effect_type: "amp".into(),
        model_id: "vst3_amp".into(),
        enabled: true,
        parameter_groups: ModelRc::new(VecModel::from(vec![
            SharedString::from("Main"),
            SharedString::from("Tone"),
            SharedString::from("Cab"),
        ])),
        active_parameter_group: 0,
        parameter_lines: ModelRc::new(VecModel::from(vec![CompactParamLine {
            cells: ModelRc::new(VecModel::default()),
        }])),
        row_height: 128.0,
        row_y: 12.0,
        ..Default::default()
    }
}

#[test]
fn clicking_a_compact_row_tab_switches_that_blocks_parameter_group() {
    i_slint_backend_testing::init_no_event_loop();

    let w = CompactChainViewWindow::new().unwrap();
    w.set_compact_blocks(ModelRc::new(VecModel::from(vec![tabbed_block()])));

    let picked: Rc<RefCell<Option<(i32, i32, i32)>>> = Rc::new(RefCell::new(None));
    let p = picked.clone();
    w.on_select_block_parameter_group(move |ci, bi, gi| *p.borrow_mut() = Some((ci, bi, gi)));
    // The page calls these while dragging; stub them so the window is complete.
    w.on_slot_at(|_| 0);
    w.on_slot_y(|_| 0.0);

    w.show().unwrap();

    assert_eq!(
        i_slint_backend_testing::ElementHandle::find_by_element_id(&w, "ParamTabBar::tab-ta")
            .count(),
        3,
        "the compact row renders one hittable tab per parameter group"
    );

    assert!(
        click_id(&w, "ParamTabBar::tab-ta", 1),
        "the 2nd tab must be hittable inside the compact row"
    );
    assert_eq!(
        *picked.borrow(),
        Some((0, 4, 1)),
        "clicking 'Tone' must ask for group 1 of block 4"
    );
}

#[test]
fn a_single_group_block_renders_no_tab_bar() {
    i_slint_backend_testing::init_no_event_loop();

    let mut block = tabbed_block();
    block.parameter_groups = ModelRc::new(VecModel::from(vec![SharedString::from("Main")]));

    let w = CompactChainViewWindow::new().unwrap();
    w.set_compact_blocks(ModelRc::new(VecModel::from(vec![block])));
    w.on_slot_at(|_| 0);
    w.on_slot_y(|_| 0.0);
    w.show().unwrap();

    assert_eq!(
        i_slint_backend_testing::ElementHandle::find_by_element_id(&w, "ParamTabBar::tab-ta")
            .count(),
        0,
        "one group needs no tab bar — the row looks like it does today"
    );
}
