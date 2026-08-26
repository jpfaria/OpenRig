//! #878 — the editor window sized an EQ block with a formula of its own
//! (`eq-extra-height` / `eq-panel-width` in `secondary_windows_block.slint`)
//! while the panel inside it laid out from the Rust policy. The two disagreed,
//! so the widget started lower than the window allowed and the frequency
//! labels were cut off by `max-height`. One source of truth: the window reads
//! the same dimensions everything else does.

use adapter_gui::block_panel_dimensions::{compute, EqWidget, PanelInputs};
use adapter_gui::{BlockEditorWindow, BlockTypePickerItem, CurveEditorPoint};
use slint::{Global, ModelRc, VecModel};

fn eq_window(bands: usize) -> BlockEditorWindow {
    let w = BlockEditorWindow::new().unwrap();
    let mut kind = BlockTypePickerItem::default();
    kind.use_panel_editor = true;
    adapter_gui::BlockEditorBridge::get(&w)
        .set_block_type_options(ModelRc::new(VecModel::from(vec![kind])));
    adapter_gui::BlockEditorBridge::get(&w).set_block_drawer_selected_type_index(0);
    let points = vec![CurveEditorPoint::default(); bands];
    adapter_gui::BlockEditorBridge::get(&w)
        .set_curve_editor_points(ModelRc::new(VecModel::from(points)));
    w
}

#[test]
fn the_window_is_the_size_the_policy_asks_for() {
    i_slint_backend_testing::init_no_event_loop();

    let dims = compute(PanelInputs {
        knob_count: 1,
        use_panel_editor: true,
        eq_widget: EqWidget::CurveEditor { bands: 8 },
    });
    let w = eq_window(8);
    w.set_panel_knob_window_width(dims.window_width_px);
    w.set_panel_knob_window_height(dims.window_height_px);
    w.set_panel_knob_inner_height(dims.inner_panel_height_px);

    assert_eq!(
        w.get_panel_height(),
        dims.window_height_px,
        "the window height ignores the policy the panel inside it lays out from"
    );
    assert_eq!(
        w.get_panel_width(),
        dims.window_width_px,
        "the window width ignores the policy"
    );
}
