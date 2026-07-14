//! #780 — (re)builds the block editor's parameter tab state from a full param
//! list. Called at EVERY point that (re)sets the editor's params — initial
//! window setup and switching the plugin/model — so changing a VST3 plugin
//! rebuilds the tabs and re-filters the grid instead of leaving the previous
//! plugin's tabs stale (the "switching plugin doesn't reload" bug).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{Model, ModelRc, SharedString, VecModel};

use crate::block_editor::{items_in_group, parameter_groups};
use crate::{BlockEditorWindow, BlockParameterItem};

/// The live grouping state for one block editor window. Held in an `Rc<RefCell>`
/// so the tab-select callback re-filters the CURRENT plugin's params, not the
/// ones captured when the window was first built.
#[derive(Default)]
pub struct TabState {
    /// Rows pinned to every tab (the synthetic model-picker row of select
    /// blocks); never a group of their own.
    pub pinned: Vec<BlockParameterItem>,
    /// The groupable params of the active plugin, full (unfiltered).
    pub full: Vec<BlockParameterItem>,
    /// Distinct group labels, in first-appearance order.
    pub groups: Vec<String>,
}

/// Rebuild the tab bar + grid for `full_items`: split off the pinned rows,
/// derive the groups, publish the group labels, reset to the first tab, and
/// show that tab's params. Idempotent — calling it again for a different plugin
/// fully replaces the previous state.
pub fn apply_param_tabs(
    win: &BlockEditorWindow,
    items_model: &Rc<VecModel<BlockParameterItem>>,
    state: &Rc<RefCell<TabState>>,
    full_items: Vec<BlockParameterItem>,
) {
    // The synthetic model-picker row (select blocks) is not a parameter group;
    // pin it to every tab so it never becomes a spurious "Select" tab.
    let (pinned, groupable): (Vec<_>, Vec<_>) = full_items
        .into_iter()
        .partition(|it| it.path.as_str() == crate::SELECT_SELECTED_BLOCK_ID);
    let groups = parameter_groups(&groupable);
    *state.borrow_mut() = TabState {
        pinned,
        full: groupable,
        groups: groups.clone(),
    };
    publish_groups(win, &groups);
    win.set_active_parameter_group(0);
    // Show the first tab (or, with <=1 group, every param — no tab bar).
    let st = state.borrow();
    let mut visible = st.pinned.clone();
    if groups.len() > 1 {
        visible.extend(items_in_group(&st.full, &groups[0]));
    } else {
        visible.extend(st.full.clone());
    }
    items_model.set_vec(visible);
}

/// The visible rows for group index `i` given the current [`TabState`]: the
/// pinned rows followed by that group's params. Out-of-range → just the pinned
/// rows. Shared by the tab-select callback.
pub fn visible_rows_for_group(state: &TabState, i: i32) -> Vec<BlockParameterItem> {
    let mut rows = state.pinned.clone();
    if let Some(group) = usize::try_from(i).ok().and_then(|idx| state.groups.get(idx)) {
        rows.extend(items_in_group(&state.full, group));
    }
    rows
}

/// Set the group-label model on the window from `groups`.
fn publish_groups(win: &BlockEditorWindow, groups: &[String]) {
    win.set_block_parameter_groups(ModelRc::from(Rc::new(VecModel::from(
        groups
            .iter()
            .map(|g| SharedString::from(g.as_str()))
            .collect::<Vec<_>>(),
    ))));
}
