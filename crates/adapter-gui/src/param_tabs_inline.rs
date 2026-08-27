//! Responsibility: drives the parameter tabs of the inline block editor.

use crate::{AppWindow, BlockEditorWindow, BlockParameterItem};
use slint::{Global, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

use crate::param_tab_grouping::{groups_and_rows, retag_for_group, TabState};

/// #819 — the inline (fullscreen/touch) editor's counterpart of
/// [`apply_param_tabs`]. It lives deep inside the AppWindow tree, so the tab
/// state travels through the `BlockParamTabs` Slint global instead of being
/// prop-drilled. Also publishes the panel height from the #500 Rust policy so
/// the inline panel stops clipping its knobs.
pub(crate) fn apply_inline_param_tabs(
    window: &AppWindow,
    items_model: &Rc<VecModel<BlockParameterItem>>,
    state: &Rc<RefCell<TabState>>,
    full_items: Vec<BlockParameterItem>,
) {
    let (groups, rows) = groups_and_rows(&full_items);
    items_model.set_vec(rows);
    let tabs = crate::BlockParamTabs::get(window);
    tabs.set_groups(ModelRc::from(Rc::new(VecModel::from(
        groups
            .iter()
            .map(|g| SharedString::from(g.as_str()))
            .collect::<Vec<_>>(),
    ))));
    tabs.set_active(0);
    state.borrow_mut().groups = groups;
    publish_inline_panel_height(window);
}

/// Re-tag the inline model for the tab at index `i` (the global's `select`).
pub(crate) fn select_inline_param_tab(
    window: &AppWindow,
    items_model: &Rc<VecModel<BlockParameterItem>>,
    state: &Rc<RefCell<TabState>>,
    i: i32,
) {
    let group = {
        let st = state.borrow();
        usize::try_from(i)
            .ok()
            .and_then(|idx| st.groups.get(idx))
            .cloned()
    };
    let Some(group) = group else {
        return;
    };
    let current: Vec<BlockParameterItem> = items_model.iter().collect();
    items_model.set_vec(retag_for_group(&current, &group));
    crate::BlockParamTabs::get(window).set_active(i);
    publish_inline_panel_height(window);
}

/// Push the #500-computed panel height into the global (mirrors
/// `block_editor_window_lifecycle::apply_panel_dimensions`, which does the same
/// for the detached window). Slint never re-derives the knob wrap math.
pub(crate) fn publish_inline_panel_height(window: &AppWindow) {
    let tabs = crate::BlockParamTabs::get(window);
    let overlay_count = crate::BlockEditorBridge::get(window)
        .get_block_knob_overlays()
        .row_count();
    let items = crate::BlockEditorBridge::get(window).get_block_parameter_items();
    let has_tabs = tabs.get_groups().row_count() > 1;
    // The grid renders only `tab_slot >= 0` rows once a tab bar exists.
    let param_count = if has_tabs {
        (0..items.row_count())
            .filter(|&i| {
                items
                    .row_data(i)
                    .map(|it| it.tab_slot >= 0)
                    .unwrap_or(false)
            })
            .count()
    } else {
        items.row_count()
    };
    // Slint hides the param grid when overlays are present.
    let knob_count = if overlay_count > 0 {
        overlay_count
    } else {
        param_count
    };
    let eq_widget = crate::block_panel_dimensions::eq_widget_for(
        crate::BlockEditorBridge::get(window)
            .get_curve_editor_points()
            .row_count(),
        crate::BlockEditorBridge::get(window)
            .get_multi_slider_points()
            .row_count(),
    );
    let type_idx = crate::BlockEditorBridge::get(window).get_block_drawer_selected_type_index();
    let types = crate::BlockEditorBridge::get(window).get_block_type_options();
    let use_panel_editor = if type_idx >= 0 {
        types
            .row_data(type_idx as usize)
            .map(|t| t.use_panel_editor)
            .unwrap_or(false)
    } else {
        true
    };
    let dims = crate::block_panel_dimensions::compute(crate::block_panel_dimensions::PanelInputs {
        knob_count,
        use_panel_editor,
        eq_widget,
    });
    // EQ-widget blocks render no tab bar (#878) — no 40px to reserve for it.
    tabs.set_panel_height(
        dims.window_height_px
            + if has_tabs && !eq_widget.is_some() {
                40.0
            } else {
                0.0
            },
    );
    // #500 inner knob-grid dimensions: BlockPanelEditor lays out the grid from
    // these, so the inline editor must get them exactly like the detached one.
    tabs.set_inner_height(dims.inner_panel_height_px);
    tabs.set_grid_cols(dims.grid_cols as i32);
    tabs.set_grid_rows(dims.grid_rows as i32);
    tabs.set_panel_width(dims.window_width_px);
}

/// Number of parameter rows the grid actually renders for the active tab (rows
/// with `tab_slot >= 0`). For a block with no tab bar (<=1 group) every row
/// shows, so this is just the row count. Drives the window sizing.
pub fn visible_param_count(win: &BlockEditorWindow) -> usize {
    let items = crate::BlockEditorBridge::get(win).get_block_parameter_items();
    if win.get_block_parameter_groups().row_count() <= 1 {
        return items.row_count();
    }
    (0..items.row_count())
        .filter(|&i| {
            items
                .row_data(i)
                .map(|it| it.tab_slot >= 0)
                .unwrap_or(false)
        })
        .count()
}
