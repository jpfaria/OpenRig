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

use slint::{Model, ModelRc, SharedString, VecModel};

use crate::block_editor::parameter_groups;
use crate::block_editor_param_items::DEFAULT_PARAM_GROUP;
use crate::{BlockEditorWindow, BlockParameterItem};

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
        usize::try_from(i).ok().and_then(|idx| st.groups.get(idx)).cloned()
    };
    let Some(group) = group else {
        return;
    };
    let current: Vec<BlockParameterItem> = items_model.iter().collect();
    items_model.set_vec(retag_for_group(&current, &group));
    win.set_active_parameter_group(i);
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
        .filter(|&i| items.row_data(i).map(|it| it.tab_slot >= 0).unwrap_or(false))
        .count()
}
