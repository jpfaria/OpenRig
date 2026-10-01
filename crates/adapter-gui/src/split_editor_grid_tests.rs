//! #328 — the split/mixer editor grid: one row per path. With A to F the user
//! found the mixer "confusing": a 4-wide grid put Level C next to Pan B.

use super::*;
use project::block::split_param_keys::{balance, level_to, mix_level, mix_pan, mix_polarity};
use project::block::split_params::{MIX_MASTER, MIX_MASTER_SUM, SPLIT_MODE};
use project::block::{SplitBlock, SplitEnd};

fn block(path_count: usize) -> SplitBlock {
    SplitBlock::with_paths(SplitEnd::Mix, vec![vec![]; path_count])
}

fn slots(grid: &SplitEditorGrid) -> Vec<(String, i32)> {
    grid.items
        .iter()
        .map(|i| (i.path.to_string(), i.tab_slot))
        .collect()
}

#[test]
fn the_mixer_puts_each_path_on_its_own_row_then_the_master() {
    let grid = split_editor_grid(&block(3), SplitEditorKind::Mixer);
    assert_eq!((grid.cols, grid.rows), (3, 4));
    assert_eq!(
        slots(&grid),
        vec![
            (mix_level(0), 0),
            (mix_pan(0), 1),
            (mix_polarity(0), 2),
            (mix_level(1), 3),
            (mix_pan(1), 4),
            (mix_polarity(1), 5),
            (mix_level(2), 6),
            (mix_pan(2), 7),
            (mix_polarity(2), 8),
            (MIX_MASTER.to_string(), 9),
            (MIX_MASTER_SUM.to_string(), 10),
        ]
    );
}

#[test]
fn the_split_puts_the_mode_on_top_then_each_path_on_its_own_row() {
    let grid = split_editor_grid(&block(3), SplitEditorKind::Split);
    assert_eq!((grid.cols, grid.rows), (2, 4));
    assert_eq!(
        slots(&grid),
        vec![
            (SPLIT_MODE.to_string(), 0),
            (level_to(0), 2),
            (balance(0), 3),
            (level_to(1), 4),
            (balance(1), 5),
            (level_to(2), 6),
            (balance(2), 7),
        ]
    );
}

#[test]
fn six_paths_make_six_path_rows() {
    let grid = split_editor_grid(&block(6), SplitEditorKind::Mixer);
    assert_eq!((grid.cols, grid.rows), (3, 7), "A to F, then the master");
}

#[test]
fn knobs_of_one_path_share_a_run() {
    let items = split_editor_items(&block(2), SplitEditorKind::Split);
    assert_eq!(
        knob_runs(&items),
        vec![0, 1, 1, 2, 2],
        "the mode alone, then path A, then path B"
    );
}
