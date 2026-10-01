//! Responsibility: drops a deleted take from the open DI panel's list.
//! The panel shows a snapshot of the chain's list taken when it
//! opened, so a deleted take would otherwise linger until it is reopened.

/// The open DI panel's list, as the `DiPanel` global holds it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DiPanelList {
    pub sources: Vec<String>,
    pub take_rows: Vec<bool>,
    pub selected: i32,
    pub playing: bool,
}

/// `list` once the take labelled `label` is gone. Only a take row is ever
/// removed. Deleting the selected take clears the selection and stops: the
/// core has just unloaded and disarmed it on this chain.
pub(crate) fn without_take(mut list: DiPanelList, label: &str) -> DiPanelList {
    let Some(row) = list
        .sources
        .iter()
        .zip(&list.take_rows)
        .position(|(source, is_take)| *is_take && source == label)
    else {
        return list;
    };
    list.sources.remove(row);
    list.take_rows.remove(row);
    let row = row as i32;
    if list.selected == row {
        list.selected = -1;
        list.playing = false;
    } else if list.selected > row {
        list.selected -= 1;
    }
    list
}

#[cfg(test)]
#[path = "di_panel_take_removal_tests.rs"]
mod tests;
