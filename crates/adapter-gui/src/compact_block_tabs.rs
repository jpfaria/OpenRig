//! #787 — which parameter tab each compact block shows.
//!
//! View state, not project state: it is not a `Command` and never reaches the
//! `.openrig`. `build_compact_blocks` re-runs on every parameter change, so the
//! selection has to live outside the model it rebuilds — hence this store,
//! keyed by block id. The compact view is a single-window, single-threaded
//! surface, so a thread-local keeps every `build_compact_blocks` call site
//! (there are eight) free of a state parameter it would only forward.

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static ACTIVE_GROUPS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Index of the tab `block_id` is showing, within `groups`. Falls back to the
/// first tab when the block never picked one, or when the group it picked is
/// gone (the model/plugin was switched) — the same rule the detached editor
/// applies when it rebuilds its tabs.
pub(crate) fn active_group_index(block_id: &str, groups: &[String]) -> usize {
    ACTIVE_GROUPS.with(|store| {
        store
            .borrow()
            .get(block_id)
            .and_then(|active| groups.iter().position(|g| g == active))
            .unwrap_or(0)
    })
}

/// Remember the tab `block_id` is showing.
pub(crate) fn set_active_group(block_id: &str, group: &str) {
    ACTIVE_GROUPS.with(|store| {
        store
            .borrow_mut()
            .insert(block_id.to_string(), group.to_string());
    });
}

/// Drop every remembered tab. Only used to isolate tests from each other.
#[cfg(test)]
pub(crate) fn reset_active_groups() {
    ACTIVE_GROUPS.with(|store| store.borrow_mut().clear());
}

#[cfg(test)]
#[path = "compact_block_tabs_tests.rs"]
mod tests;
