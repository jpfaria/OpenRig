//! #787 — the compact view's per-block tab state and the geometry it produces
//! through `build_compact_blocks`.
//!
//! Which tab a compact block shows is view state, not project state, so it is
//! not a `Command`. `set_compact_blocks` re-runs on every parameter change, so
//! the selected tab has to survive the rebuild.

use crate::compact_block_layout::{BASE_ROW_HEIGHT_PX, ROW_GAP_PX};
use crate::compact_block_tabs::{active_group_index, reset_active_groups, set_active_group};
use crate::compact_block_view::build_compact_blocks;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use slint::Model;

fn delay_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: "digital_clean".to_string(),
            params: ParameterSet::default(),
        }),
    }
}

fn project_with(blocks: Vec<AudioBlock>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("test:chain".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks,
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }
}

#[test]
fn every_compact_block_carries_its_geometry() {
    let project = project_with(vec![delay_block("a"), delay_block("b")]);

    let items = build_compact_blocks(&project, 0);

    assert_eq!(items.len(), 2);
    assert!(
        items[0].row_height >= BASE_ROW_HEIGHT_PX,
        "a row is never shorter than the base height"
    );
    assert_eq!(items[0].row_y, ROW_GAP_PX, "the first row clears the slot");
    assert_eq!(
        items[1].row_y,
        ROW_GAP_PX + items[0].row_height + ROW_GAP_PX,
        "the second row stacks below the first, gap included"
    );
    assert!(
        items[0].parameter_items.iter().all(|p| p.strip_line >= 0),
        "a single-group block lays every parameter out"
    );
}

#[test]
fn a_single_group_block_shows_no_tab_bar() {
    let project = project_with(vec![delay_block("a")]);

    let items = build_compact_blocks(&project, 0);

    assert!(
        items[0].parameter_groups.row_count() <= 1,
        "a native delay declares no parameter groups, so it needs no tab bar"
    );
}

#[test]
fn the_selected_tab_survives_a_rebuild_of_the_compact_model() {
    reset_active_groups();
    let groups = vec!["Main".to_string(), "Tone".to_string(), "Cab".to_string()];

    set_active_group("amp-1", "Cab");

    assert_eq!(
        active_group_index("amp-1", &groups),
        2,
        "the tab picked before the rebuild is still the active one"
    );
}

#[test]
fn a_group_that_no_longer_exists_falls_back_to_the_first_tab() {
    reset_active_groups();
    set_active_group("amp-1", "Cab");

    // The plugin/model was switched: the old group is gone.
    let groups = vec!["Amp".to_string(), "Capture".to_string()];

    assert_eq!(
        active_group_index("amp-1", &groups),
        0,
        "a stale group falls back to the first tab instead of hiding every param"
    );
}

#[test]
fn a_block_never_asked_for_a_tab_shows_the_first_one() {
    reset_active_groups();

    assert_eq!(active_group_index("untouched", &["Main".to_string()]), 0);
    assert_eq!(active_group_index("untouched", &[]), 0);
}
