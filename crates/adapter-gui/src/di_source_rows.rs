//! Responsibility: keeps every chain row's DI source list current.
//! #827 — the DI picker's entries (bundled loops, saved takes, the loaded file)
//! and its highlighted row, refreshed for EVERY chain on the meter tick —
//! running or not, like the DI output select (#808): a DI-only chain is never
//! enabled, and must still see a take saved after the project opened.

use std::path::PathBuf;
use std::rc::Rc;

use application::di_loader::DiLoopSource;
use domain::ids::ChainId;
use project::project::Project;
use slint::{Model, ModelRc, SharedString, VecModel};

use crate::ProjectChainItem;

/// Rewrite each row's DI sources / selected index only where they changed.
pub(crate) fn apply_di_sources_to_rows(
    model: &VecModel<ProjectChainItem>,
    project: &Project,
    loaded_source: impl Fn(&ChainId) -> Option<DiLoopSource>,
    bundled_ids: &[String],
    takes: &[PathBuf],
) {
    let bundled_refs: Vec<&str> = bundled_ids.iter().map(String::as_str).collect();
    for (idx, chain) in project.chains.iter().enumerate() {
        let Some(mut row) = model.row_data(idx) else {
            continue;
        };
        // #661: the loaded source comes from the dispatcher, so a hand-picked
        // file is listed and the active entry stays highlighted on reopen.
        let loaded = loaded_source(&chain.id);
        let desired = crate::di_loop_ui_sources::build_di_loop_sources_with_takes(
            &bundled_refs,
            takes,
            loaded.as_ref(),
        );
        let selected = loaded.as_ref().map_or(-1, |source| {
            crate::di_loop_ui_sources::di_loop_selected_index(&desired, source)
        });
        let current: Vec<String> = row.di_loop_sources.iter().map(|s| s.to_string()).collect();
        let sources_changed = current != desired;
        let selected_changed = row.di_loop_selected_index != selected;
        if !sources_changed && !selected_changed {
            continue;
        }
        if sources_changed {
            row.di_loop_sources = ModelRc::from(Rc::new(VecModel::from(
                desired
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            )));
        }
        if selected_changed {
            row.di_loop_selected_index = selected;
        }
        model.set_row_data(idx, row);
    }
}

#[cfg(test)]
#[path = "di_source_rows_tests.rs"]
mod tests;
