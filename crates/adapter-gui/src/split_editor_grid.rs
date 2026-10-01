//! Responsibility: places a split editor's knobs on its grid.
//!
//! #328: one row per path. A path's knobs are a run; the knobs of the whole
//! split (mode; master, master sum) are runs of their own, so the mode sits
//! alone above the paths and the master below them. Each run starts a row of
//! the grid, addressed by the item's `tab_slot`. With A to F a plain 4-wide
//! wrap put Level C beside Pan B and the user could not read the mixer.

use project::block::split_param_keys::path_of_key;
use project::block::SplitBlock;

use crate::split_editor_items::{split_editor_items, SplitEditorKind};
use crate::BlockParameterItem;

pub(crate) struct SplitEditorGrid {
    pub(crate) items: Vec<BlockParameterItem>,
    pub(crate) cols: usize,
    pub(crate) rows: usize,
}

/// The run each item belongs to: consecutive knobs of one path, or of the
/// whole split, share a run.
pub(crate) fn knob_runs(items: &[BlockParameterItem]) -> Vec<usize> {
    let mut runs = Vec::with_capacity(items.len());
    let mut previous: Option<Option<usize>> = None;
    let mut run = 0;
    for item in items {
        let owner = path_of_key(item.path.as_str()).map(|(_, at)| at);
        if previous.is_some_and(|p| p != owner) {
            run += 1;
        }
        previous = Some(owner);
        runs.push(run);
    }
    runs
}

pub(crate) fn split_editor_grid(split: &SplitBlock, kind: SplitEditorKind) -> SplitEditorGrid {
    let mut items = split_editor_items(split, kind);
    let runs = knob_runs(&items);
    let rows = runs.last().map_or(0, |last| last + 1);
    let cols = (0..rows)
        .map(|row| runs.iter().filter(|&&r| r == row).count())
        .max()
        .unwrap_or(0)
        .max(1);
    let mut col = 0;
    for (index, item) in items.iter_mut().enumerate() {
        if index > 0 && runs[index] != runs[index - 1] {
            col = 0;
        }
        item.tab_slot = (runs[index] * cols + col) as i32;
        col += 1;
    }
    SplitEditorGrid {
        items,
        cols,
        rows: rows.max(1),
    }
}

#[cfg(test)]
#[path = "split_editor_grid_tests.rs"]
mod tests;
