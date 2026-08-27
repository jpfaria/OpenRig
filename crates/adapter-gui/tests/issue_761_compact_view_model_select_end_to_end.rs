//! Issue #761, root-cause-confirmed follow-up: `issue_761_model_select_popup_interaction.rs`
//! proved `ModelSelectWithSearch`'s row click never fires `select` because
//! its option list renders inside a `PopupWindow` — a separate surface that
//! doesn't receive dispatched pointer events (same defect class as #749).
//!
//! `CompactBlockRow`/`ModelSelectWithSearch` live inside `CompactBlockRow`'s
//! `clip: true` root and a `Flickable` (`compact_chain_view.slint`), so an
//! inline replacement popup would just get clipped at the 100px row height.
//! #749 solved the identical problem for the DI loop panel by rendering the
//! panel as a single overlay at its window's own root (see `DiPanel` /
//! `app-window.slint`) instead of nested inside the clipped chain list.
//!
//! Intended behavior for #761: the SAME pattern, scoped to
//! `CompactChainViewPage`'s own root (a separate `CompactChainViewWindow`,
//! not `AppWindow` — #749's app-level `DiPanel` overlay doesn't reach it).
//! This test drives the real `CompactChainViewPage` end-to-end: open a
//! multi-model block's picker, click a DIFFERENT model row, and assert
//! `choose-model-by-id` fires with that model's id — proving the click
//! actually reaches a row that isn't trapped inside a `PopupWindow`.

use adapter_gui::{BlockModelPickerItem, CompactBlockItem, CompactChainViewWindow};
use i_slint_backend_testing::ElementHandle;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn model(model_id: &str, display: &str) -> BlockModelPickerItem {
    BlockModelPickerItem {
        effect_type: "reverb".into(),
        model_id: model_id.into(),
        label: display.into(),
        display_name: display.into(),
        subtitle: SharedString::default(),
        icon_kind: "reverb".into(),
        brand: SharedString::default(),
        type_label: "NATIVE".into(),
        panel_bg: slint::Color::from_argb_u8(255, 0, 0, 0),
        panel_text: slint::Color::from_argb_u8(255, 255, 255, 255),
        brand_strip_bg: slint::Color::from_argb_u8(255, 0, 0, 0),
        model_font: SharedString::default(),
        available: true,
        thumbnail_path: SharedString::default(),
    }
}

fn reverb_block(chain_index: i32, block_index: i32) -> CompactBlockItem {
    let models = vec![
        model("dattorro_plate", "Dattorro Plate"),
        model("lv2_dragonfly_plate", "Dragonfly Plate"),
    ];
    CompactBlockItem {
        chain_index,
        block_index,
        display_label: "RVB".into(),
        model_id: "lv2_dragonfly_plate".into(),
        model_selected_index: 1,
        models: ModelRc::from(Rc::new(VecModel::from(models.clone()))),
        filtered_models: ModelRc::from(Rc::new(VecModel::from(models))),
        enabled: true,
        // #787: the row sizes itself from the geometry `build_compact_blocks`
        // computes — without it the row would be 0px tall and nothing in it is
        // hittable.
        row_height: 100.0,
        row_y: 12.0,
        ..Default::default()
    }
}

fn click_id(w: &impl ComponentHandle, id: &str, nth: usize) -> bool {
    let Some(el) = ElementHandle::find_by_element_id(w, id).nth(nth) else {
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
fn compact_view_model_row_click_fires_choose_model_by_id_with_the_clicked_model() {
    i_slint_backend_testing::init_no_event_loop();

    let w = CompactChainViewWindow::new().unwrap();
    w.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(vec![reverb_block(
        0, 3,
    )]))));

    let picked: Rc<RefCell<Option<(i32, i32, String)>>> = Rc::new(RefCell::new(None));
    let p = picked.clone();
    w.on_choose_block_model_by_id(move |ci, bi, model_id| {
        *p.borrow_mut() = Some((ci, bi, model_id.to_string()))
    });

    w.show().unwrap();

    // 1. Open the RVB block's model picker.
    assert!(
        click_id(&w, "ModelSelectWithSearch::main-ta", 0),
        "the block's model-select button must be hittable"
    );

    // 2. Click the OTHER model (dattorro_plate) — must NOT be trapped in an
    //    unreachable PopupWindow surface (#761 root cause).
    assert!(
        click_id(&w, "ModelSelectWithSearch::row-ta", 0)
            || click_id(&w, "CompactModelSelectOverlay::row-ta", 0),
        "a model row must be hittable from the real compact view page"
    );

    assert_eq!(
        picked.borrow().clone(),
        Some((0, 3, "dattorro_plate".to_string())),
        "REGRESSION #761: clicking a different model in the compact view's \
         RVB row must fire choose-model-by-id(chain_index, block_index, \
         clicked_model_id) — this is the exact reported bug: the list \
         opens, a different plugin is clicked, and nothing happens because \
         the row click never reaches the popup's TouchArea"
    );
}
