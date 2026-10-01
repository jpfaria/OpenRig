//! Responsibility: carries a chain volume moved from outside the GUI onto its chain card in place.
//!
//! #1007 — when a batch holds only fader steps the chain list is not
//! re-projected (see `view_refresh_policy`), so the one value a card draws
//! from a fader — the chain volume behind the speaker button — is written
//! onto that card's row. `set_row_data` keeps the card on screen; a model
//! reset would rebuild it.

use slint::{Model, VecModel};

use application::event::Event;
use project::project::Project;

use crate::ProjectChainItem;

/// Rows map 1:1 to `project.chains`, the order `replace_project_chains`
/// projects them in.
pub(crate) fn patch_chain_volumes(
    rows: &VecModel<ProjectChainItem>,
    project: &Project,
    events: &[Event],
) {
    for event in events {
        let Event::ChainVolumeChanged { chain, .. } = event else {
            continue;
        };
        let Some(index) = project.chains.iter().position(|c| &c.id == chain) else {
            continue;
        };
        let Some(mut row) = rows.row_data(index) else {
            continue;
        };
        let volume = project.chains[index].volume.round() as i32;
        if row.volume != volume {
            row.volume = volume;
            rows.set_row_data(index, row);
        }
    }
}
