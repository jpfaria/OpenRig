//! Responsibility: rebuilds the block editor's parameter tabs.
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

pub(crate) use crate::param_tab_grouping::groups_and_rows;
pub use crate::param_tab_grouping::{retag_all, retag_for_group, TabState};
pub use crate::param_tabs_inline::visible_param_count;
pub(crate) use crate::param_tabs_inline::{
    apply_inline_param_tabs, publish_inline_panel_height, select_inline_param_tab,
};
use crate::{BlockEditorWindow, BlockParameterItem};

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
    let (groups, rows) = groups_and_rows(&full_items);
    items_model.set_vec(rows);
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
