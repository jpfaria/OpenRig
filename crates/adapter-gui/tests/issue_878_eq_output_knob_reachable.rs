//! #878 — an EQ widget draws the band parameters, NOT every parameter: the
//! 8-Band Parametric EQ also carries an `output_db` trim, which is a normal
//! knob. Hiding the whole parameter grid behind "this block has an EQ widget"
//! made that knob unreachable — the old tab bar's OUTPUT tab opened an empty
//! panel. The panel must keep rendering the knobs the widget does NOT own, in
//! the panel strip above the widget.

use adapter_gui::{BlockEditorWindow, BlockParameterItem, BlockTypePickerItem, CurveEditorPoint};
use slint::{ComponentHandle, Global, ModelRc, VecModel};

/// Bottom of the block panel strip: the header (48px) plus the strip an EQ
/// block gets. A knob below this line would sit on top of the EQ widget.
const PANEL_BOTTOM_PX: f32 = 140.0;

fn panel_editor_window() -> BlockEditorWindow {
    let w = BlockEditorWindow::new().unwrap();
    let mut kind = BlockTypePickerItem::default();
    kind.use_panel_editor = true;
    adapter_gui::BlockEditorBridge::get(&w)
        .set_block_type_options(ModelRc::new(VecModel::from(vec![kind])));
    adapter_gui::BlockEditorBridge::get(&w).set_block_drawer_selected_type_index(0);
    w
}

/// A row the EQ widget owns — it carries no widget of its own.
fn widget_owned(path: &str) -> BlockParameterItem {
    let mut it = BlockParameterItem::default();
    it.path = path.into();
    it.widget_kind = "".into();
    it.tab_slot = -1;
    it
}

/// A plain knob row — the EQ's output trim.
fn knob(path: &str, slot: i32) -> BlockParameterItem {
    let mut it = BlockParameterItem::default();
    it.path = path.into();
    it.label = path.into();
    it.widget_kind = "knob".into();
    it.numeric_min = -24.0;
    it.numeric_max = 12.0;
    it.numeric_step = 0.1;
    it.tab_slot = slot;
    it
}

#[test]
fn eq_block_keeps_the_knobs_its_widget_does_not_draw() {
    i_slint_backend_testing::init_no_event_loop();

    let w = panel_editor_window();
    let bands = vec![CurveEditorPoint::default(); 4];
    adapter_gui::BlockEditorBridge::get(&w)
        .set_curve_editor_points(ModelRc::new(VecModel::from(bands)));
    adapter_gui::BlockEditorBridge::get(&w).set_block_parameter_items(ModelRc::new(
        VecModel::from(vec![
            widget_owned("band1_freq"),
            widget_owned("band1_gain"),
            knob("output_db", 0),
        ]),
    ));
    w.set_panel_grid_cols(1);
    w.show().unwrap();

    let knobs: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_element_id(&w, "PanelKnob::ta").collect();
    assert_eq!(
        knobs.len(),
        1,
        "the EQ's output trim is not drawn by the curve editor — the panel must render it"
    );

    let bottom = knobs[0].absolute_position().y + knobs[0].size().height;
    assert!(
        bottom <= PANEL_BOTTOM_PX,
        "the knob must sit in the panel strip, not on top of the EQ widget (bottom was {bottom})"
    );
}
