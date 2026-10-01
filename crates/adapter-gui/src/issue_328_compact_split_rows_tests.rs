//! #328 — the compact view of a chain with a split. The blocks inside the
//! split's paths are chain rows like any other: the user saw CAB, FILTER and
//! DYN (path A of a Y split) vanish from the compact view, and the SPLIT row
//! offered an empty model selector.

use slint::Model;

use project::chain::Chain;
use project::project::Project;

use crate::chain_graph_fixtures_tests::{mix_chain, y_chain};
use crate::compact_block_view::build_compact_blocks;
use crate::CompactBlockItem;

fn project_of(chain: Chain) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    }
}

fn rows_of(chain: Chain) -> Vec<CompactBlockItem> {
    build_compact_blocks(&project_of(chain), 0, &[])
}

fn ids(rows: &[CompactBlockItem]) -> Vec<String> {
    rows.iter().map(|r| r.block_id.to_string()).collect()
}

#[test]
fn a_y_split_lists_the_blocks_of_both_paths() {
    assert_eq!(
        ids(&rows_of(y_chain())),
        vec!["pre", "sp", "a1", "b1"],
        "every block of the chain is a compact row, in signal order: the split, \
         then path A, then path B"
    );
}

#[test]
fn a_mix_split_lists_its_paths_and_the_blocks_after_it() {
    assert_eq!(
        ids(&rows_of(mix_chain())),
        vec!["pre", "sp", "a1", "a2", "b1", "post"],
    );
}

#[test]
fn the_split_row_offers_no_model_to_pick() {
    for chain in [y_chain(), mix_chain()] {
        let rows = rows_of(chain);
        let split = rows
            .iter()
            .find(|r| r.block_id == "sp")
            .expect("the split is a compact row");
        assert_eq!(
            split.model_labels.row_count(),
            0,
            "a split has no model: its row must not show a model selector"
        );
    }
}

#[test]
fn each_path_row_says_which_path_it_is_on() {
    let rows = rows_of(mix_chain());
    let paths: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.block_id.to_string(), r.path_label.to_string()))
        .collect();
    assert_eq!(
        paths,
        vec![
            ("pre".into(), "".into()),
            ("sp".into(), "".into()),
            ("a1".into(), "A".into()),
            ("a2".into(), "A".into()),
            ("b1".into(), "B".into()),
            ("post".into(), "".into()),
        ]
    );
}

fn split_row(chain: Chain) -> CompactBlockItem {
    rows_of(chain)
        .into_iter()
        .find(|r| r.block_id == "sp")
        .expect("the split is a compact row")
}

fn knob_paths(row: &CompactBlockItem) -> Vec<String> {
    row.parameter_items
        .iter()
        .filter(|p| p.strip_line >= 0)
        .map(|p| p.path.to_string())
        .collect()
}

#[test]
fn a_y_split_row_shows_the_split_knobs() {
    let row = split_row(y_chain());
    assert_eq!(
        knob_paths(&row),
        vec![
            "split_mode",
            "level_to_0",
            "balance_0",
            "level_to_1",
            "balance_1"
        ],
        "the knobs the split editor shows, on the row like any block"
    );
    assert!(row.parameter_lines.row_count() > 0, "the strip is drawn");
}

#[test]
fn a_mix_split_row_has_a_split_tab_and_a_mixer_tab() {
    let row = split_row(mix_chain());
    let tabs: Vec<String> = row.parameter_groups.iter().map(|g| g.to_string()).collect();
    assert_eq!(
        tabs,
        vec!["Split", "Mixer"],
        "tab labels read like every other block's groups"
    );
    assert_eq!(row.active_parameter_group, 0);
    assert_eq!(
        knob_paths(&row),
        vec![
            "split_mode",
            "level_to_0",
            "balance_0",
            "level_to_1",
            "balance_1"
        ],
        "the first tab is the split"
    );
}

fn line_of(row: &CompactBlockItem, path: &str) -> i32 {
    row.parameter_items
        .iter()
        .find(|p| p.path.as_str() == path)
        .map(|p| p.strip_line)
        .expect("the knob is on the row")
}

/// #328: with A to F the strip wrapped mid-path, so Level E sat on one line
/// and Balance E on the next. A path's knobs stay on one line.
#[test]
fn a_six_path_split_row_never_breaks_a_path_across_lines() {
    use crate::chain_graph_fixtures_tests::{chain, core, split_paths};
    use project::block::SplitEnd;
    let row = split_row(chain(vec![
        core("pre"),
        split_paths("sp", SplitEnd::Mix, vec![vec![]; 6]),
    ]));
    assert!(row.parameter_lines.row_count() > 1, "six paths wrap");
    for i in 0..6 {
        assert_eq!(
            line_of(&row, &format!("level_to_{i}")),
            line_of(&row, &format!("balance_{i}")),
            "path {i} is split across two lines"
        );
    }
}

#[test]
fn a_two_path_split_row_keeps_one_line() {
    assert_eq!(split_row(mix_chain()).parameter_lines.row_count(), 1);
}
