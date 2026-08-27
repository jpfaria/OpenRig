//! Responsibility: groups a block's parameters into the tabs they belong to.

use crate::block_editor::parameter_groups;
use crate::block_editor_param_items::DEFAULT_PARAM_GROUP;
use crate::BlockParameterItem;

/// The live tab labels for one block editor window, in first-appearance order.
/// Held in an `Rc<RefCell>` so the tab-select callback maps a clicked index to
/// the CURRENT plugin's group, not the one captured when the window was built.
#[derive(Default)]
pub struct TabState {
    pub groups: Vec<String>,
}

/// The tab group a parameter belongs to (empty → the default tab).
pub(crate) fn group_label(it: &BlockParameterItem) -> &str {
    let g = it.group.as_str();
    if g.is_empty() {
        DEFAULT_PARAM_GROUP
    } else {
        g
    }
}

/// Whether a row is the synthetic model-picker (select blocks): it is pinned to
/// every tab and is never a group of its own.
pub(crate) fn is_pinned(it: &BlockParameterItem) -> bool {
    it.path.as_str() == crate::SELECT_SELECTED_BLOCK_ID
}

/// Return `items` (FULL, order preserved) with each row's `tab_slot` set: the
/// pinned rows and the rows of the `active` group get a running 0-based slot;
/// every other row gets -1 (hidden). Values are preserved — only `tab_slot`
/// changes — so switching tabs never loses an edit.
///
/// A row with no widget of its own is drawn by the block's EQ widget, never by
/// the grid (#878) — it is hidden so it does not eat a slot and push the rows
/// that DO render (an EQ's output trim) out of the panel.
pub fn retag_for_group(items: &[BlockParameterItem], active: &str) -> Vec<BlockParameterItem> {
    retag(items, |it| is_pinned(it) || group_label(it) == active)
}

/// Tag the rows of the groups the EQ widget does NOT draw, with no tab filter —
/// what a block whose widget draws the rest needs: there is no tab bar to pick
/// a group with, and the handful of remaining knobs form a single dense strip.
///
/// A group the widget draws is drawn WHOLE: a band's on/off toggle and filter
/// type belong to that band, so the widget owns them along with its curve. An
/// equalizer shows sliders — a loose row of ENABLED toggles and TYPE dropdowns
/// above them is not something an equalizer has (#878).
pub fn retag_all(items: &[BlockParameterItem]) -> Vec<BlockParameterItem> {
    let widget_groups: Vec<&str> = items
        .iter()
        .filter(|it| it.widget_kind.is_empty())
        .map(group_label)
        .collect();
    retag(items, |it| !widget_groups.contains(&group_label(it)))
}

pub(crate) fn retag(
    items: &[BlockParameterItem],
    in_tab: impl Fn(&BlockParameterItem) -> bool,
) -> Vec<BlockParameterItem> {
    let mut slot = 0i32;
    items
        .iter()
        .map(|it| {
            let mut out = it.clone();
            let visible = !it.widget_kind.is_empty() && in_tab(it);
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

/// Whether an EQ widget (CurveEditor / MultiSlider) draws part of this block's
/// parameters: those rows carry no widget of their own. Such a block gets no
/// tab bar — the widget already shows every band at once (#878).
pub(crate) fn drawn_by_eq_widget(items: &[BlockParameterItem]) -> bool {
    items.iter().any(|it| it.widget_kind.is_empty())
}

/// The tab labels to publish for a fresh parameter list, plus the same list
/// tagged for its first tab. An EQ-widget block gets no labels at all and shows
/// every grid-owned row at once.
pub(crate) fn groups_and_rows(
    full_items: &[BlockParameterItem],
) -> (Vec<String>, Vec<BlockParameterItem>) {
    if drawn_by_eq_widget(full_items) {
        return (Vec::new(), retag_all(full_items));
    }
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
    let rows = retag_for_group(full_items, active);
    (groups, rows)
}
