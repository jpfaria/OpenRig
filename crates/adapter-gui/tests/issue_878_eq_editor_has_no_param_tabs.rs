//! #878 — a block whose parameters are drawn by an EQ widget (CurveEditor for
//! the parametric EQs, MultiSlider for the graphic ones) owns its ENTIRE
//! parameter surface: the knob grid is hidden for it. The parameter tab bar
//! only ever filters that grid, so for such a block the tabs are inert — and
//! the 40px bar pushes the widget down until its bottom sliders are clipped.
//! The editor must therefore render NO tab bar for EQ-widget blocks, while
//! every other grouped block keeps its tabs.

use adapter_gui::{BlockEditorWindow, BlockTypePickerItem, CurveEditorPoint, MultiSliderPoint};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

/// An editor window showing the visual panel editor — the surface that owns the
/// tab bar. Without a selected block type the window falls back to the form
/// editor, which has no tabs at all and would make every assertion vacuous.
fn panel_editor_window() -> BlockEditorWindow {
    let w = BlockEditorWindow::new().unwrap();
    let mut kind = BlockTypePickerItem::default();
    kind.use_panel_editor = true;
    kind.label = SharedString::from("FILTER");
    w.set_block_type_options(ModelRc::new(VecModel::from(vec![kind])));
    w.set_block_drawer_selected_type_index(0);
    w
}

fn groups(labels: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        labels
            .iter()
            .map(|l| SharedString::from(*l))
            .collect::<Vec<_>>(),
    ))
}

fn tab_count(w: &impl ComponentHandle) -> usize {
    i_slint_backend_testing::ElementHandle::find_by_element_id(w, "ParamTabBar::tab-ta").count()
}

#[test]
fn curve_editor_block_renders_no_parameter_tabs() {
    i_slint_backend_testing::init_no_event_loop();

    let w = panel_editor_window();
    w.set_block_parameter_groups(groups(&["Band 1", "Band 2", "Band 3", "Output"]));
    let bands = vec![CurveEditorPoint::default(); 4];
    w.set_curve_editor_points(ModelRc::new(VecModel::from(bands)));
    w.show().unwrap();

    assert_eq!(
        tab_count(&w),
        0,
        "a curve-editor block draws every parameter itself — no tab bar"
    );
}

#[test]
fn multi_slider_block_renders_no_parameter_tabs() {
    i_slint_backend_testing::init_no_event_loop();

    let w = panel_editor_window();
    w.set_block_parameter_groups(groups(&["Low", "Mid", "High"]));
    let bands = vec![MultiSliderPoint::default(); 3];
    w.set_multi_slider_points(ModelRc::new(VecModel::from(bands)));
    w.show().unwrap();

    assert_eq!(
        tab_count(&w),
        0,
        "a multi-slider block draws every parameter itself — no tab bar"
    );
}

/// The symptom the tabs caused: their 40px shifted the widget down while it
/// kept sizing itself off the window height, so the band sliders ran past the
/// bottom edge and the window clipped them.
#[test]
fn every_band_slider_fits_inside_the_editor_window() {
    i_slint_backend_testing::init_no_event_loop();

    // Size the window exactly as production does — off the Rust policy, not a
    // number picked here: the clipping is a mismatch between the window the
    // policy asks for and the space the widget needs (#878).
    let dims = adapter_gui::block_panel_dimensions::compute(
        adapter_gui::block_panel_dimensions::PanelInputs {
            knob_count: 1,
            use_panel_editor: true,
            eq_widget: adapter_gui::block_panel_dimensions::EqWidget::CurveEditor { bands: 8 },
        },
    );
    let window_h = dims.window_height_px;
    let w = panel_editor_window();
    w.set_block_parameter_groups(groups(&[
        "Band 1", "Band 2", "Band 3", "Band 4", "Band 5", "Band 6", "Band 7", "Band 8", "Output",
    ]));
    let bands = vec![CurveEditorPoint::default(); 8];
    w.set_curve_editor_points(ModelRc::new(VecModel::from(bands)));
    w.set_panel_knob_inner_height(dims.inner_panel_height_px);
    w.window()
        .set_size(slint::LogicalSize::new(dims.window_width_px, window_h));
    w.show().unwrap();

    let sliders: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_element_id(&w, "EqBandSlider::ta")
            .collect();
    assert_eq!(sliders.len(), 8, "one slider per band");
    for (i, s) in sliders.iter().enumerate() {
        let bottom = s.absolute_position().y + s.size().height;
        assert!(
            bottom <= window_h,
            "band {i} runs {}px past the bottom of the window",
            bottom - window_h
        );
    }
}

#[test]
fn grouped_block_without_eq_widget_keeps_its_tabs() {
    i_slint_backend_testing::init_no_event_loop();

    let w = panel_editor_window();
    w.set_block_parameter_groups(groups(&["Tone", "Voicing", "Drive"]));
    w.show().unwrap();

    assert_eq!(
        tab_count(&w),
        3,
        "blocks that render the knob grid still need the tab bar"
    );
}
