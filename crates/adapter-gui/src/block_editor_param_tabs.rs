//! #780 — (re)builds the block editor's parameter tab state from a full param
//! list. Called at EVERY point that (re)sets the editor's params — initial
//! window setup and switching the plugin/model — so changing a VST3 plugin
//! rebuilds the tabs instead of leaving the previous plugin's tabs stale.
//!
//! Crucially, `block-parameter-items` is kept FULL (every tab's params); the
//! active tab is expressed as a per-item `tab_slot` (0-based slot, or -1 when
//! hidden). The grid renders only `tab_slot >= 0`. Keeping the model full is
//! what makes save correct: persistence builds the block from this model, so a
//! filtered model would drop the non-active tabs' params (#780).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Global, Model, ModelRc, SharedString, VecModel};

use crate::block_editor::parameter_groups;
use crate::block_editor_param_items::DEFAULT_PARAM_GROUP;
use crate::{AppWindow, BlockEditorWindow, BlockParameterItem};

/// The live tab labels for one block editor window, in first-appearance order.
/// Held in an `Rc<RefCell>` so the tab-select callback maps a clicked index to
/// the CURRENT plugin's group, not the one captured when the window was built.
#[derive(Default)]
pub struct TabState {
    pub groups: Vec<String>,
}

/// The tab group a parameter belongs to (empty → the default tab).
fn group_label(it: &BlockParameterItem) -> &str {
    let g = it.group.as_str();
    if g.is_empty() {
        DEFAULT_PARAM_GROUP
    } else {
        g
    }
}

/// Whether a row is the synthetic model-picker (select blocks): it is pinned to
/// every tab and is never a group of its own.
fn is_pinned(it: &BlockParameterItem) -> bool {
    it.path.as_str() == crate::SELECT_SELECTED_BLOCK_ID
}

/// Return `items` (FULL, order preserved) with each row's `tab_slot` set: the
/// pinned rows and the rows of the `active` group get a running 0-based slot;
/// every other row gets -1 (hidden). Values are preserved — only `tab_slot`
/// changes — so switching tabs never loses an edit.
pub fn retag_for_group(items: &[BlockParameterItem], active: &str) -> Vec<BlockParameterItem> {
    let mut slot = 0i32;
    items
        .iter()
        .map(|it| {
            let mut out = it.clone();
            let visible = is_pinned(it) || group_label(it) == active;
            out.tab_slot = if visible {
                let s = slot;
                slot += 1;
                s
            } else {
                -1
            };
            out
        })
        .collect()
}

/// Rebuild the tab bar + grid for `full_items`: derive the groups, publish the
/// labels, reset to the first tab, and tag every row's `tab_slot` for that tab
/// while keeping the model FULL. Idempotent — calling it again for a different
/// plugin fully replaces the previous state.
pub fn apply_param_tabs(
    win: &BlockEditorWindow,
    items_model: &Rc<VecModel<BlockParameterItem>>,
    state: &Rc<RefCell<TabState>>,
    full_items: Vec<BlockParameterItem>,
) {
    let groupable: Vec<BlockParameterItem> = full_items
        .iter()
        .filter(|it| !is_pinned(it))
        .cloned()
        .collect();
    let groups = parameter_groups(&groupable);
    let active = groups
        .first()
        .map(String::as_str)
        .unwrap_or(DEFAULT_PARAM_GROUP);
    items_model.set_vec(retag_for_group(&full_items, active));
    win.set_block_parameter_groups(ModelRc::from(Rc::new(VecModel::from(
        groups
            .iter()
            .map(|g| SharedString::from(g.as_str()))
            .collect::<Vec<_>>(),
    ))));
    win.set_active_parameter_group(0);
    state.borrow_mut().groups = groups;
}

/// Re-tag the (full) model in `items_model` for the group at index `i`. Reads
/// the current rows (so live edits survive), so it is the tab-select action.
pub fn select_param_tab(
    win: &BlockEditorWindow,
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
    win.set_active_parameter_group(i);
}

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
    let groupable: Vec<BlockParameterItem> = full_items
        .iter()
        .filter(|it| !is_pinned(it))
        .cloned()
        .collect();
    let groups = parameter_groups(&groupable);
    let active = groups
        .first()
        .map(String::as_str)
        .unwrap_or(DEFAULT_PARAM_GROUP);
    items_model.set_vec(retag_for_group(&full_items, active));
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
    let overlay_count = window.get_block_knob_overlays().row_count();
    let items = window.get_block_parameter_items();
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
    let has_eq_widget = window.get_multi_slider_points().row_count() > 0
        || window.get_curve_editor_points().row_count() > 0;
    let type_idx = window.get_block_drawer_selected_type_index();
    let types = window.get_block_type_options();
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
        has_eq_widget,
    });
    tabs.set_panel_height(dims.window_height_px + if has_tabs { 40.0 } else { 0.0 });
}

/// Number of parameter rows the grid actually renders for the active tab (rows
/// with `tab_slot >= 0`). For a block with no tab bar (<=1 group) every row
/// shows, so this is just the row count. Drives the window sizing.
pub fn visible_param_count(win: &BlockEditorWindow) -> usize {
    let items = win.get_block_parameter_items();
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
