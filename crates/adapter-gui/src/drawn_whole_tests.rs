//! #915 — what a block WITHOUT a tab bar draws: all of it. Which blocks have
//! a tab bar is decided in `tab_worthy_groups_tests`; here the point is that
//! dropping the bar must never drop a parameter with it. Both surfaces that
//! render parameters are covered, because they used to derive the tab state
//! separately and could disagree.

use crate::block_editor::block_parameter_items_for_model;
use crate::param_tab_grouping::groups_and_rows;
use crate::BlockParameterItem;
use project::param::ParameterSet;

fn items(effect_type: &str, model: &str) -> Vec<BlockParameterItem> {
    block_parameter_items_for_model(effect_type, model, &ParameterSet::default())
}

fn drawn(rows: &[BlockParameterItem]) -> Vec<String> {
    rows.iter()
        .filter(|it| it.tab_slot >= 0)
        .map(|it| it.path.to_string())
        .collect()
}

#[test]
fn a_native_amp_draws_every_knob_at_once() {
    let (groups, rows) = groups_and_rows(&items("amp", "blackface_clean"));
    assert!(groups.is_empty());
    assert_eq!(
        drawn(&rows),
        vec![
            "input", "gain", "bass", "middle", "treble", "master", "bright", "sag", "room_mix",
            "output",
        ],
        "every parameter is on the one panel, in schema order"
    );
}

#[test]
fn a_tabbed_block_draws_the_active_group_only() {
    let (groups, rows) = groups_and_rows(&items("wah", "cry_classic"));
    assert_eq!(groups, vec!["Wah", "Output"]);
    assert_eq!(
        drawn(&rows).len(),
        2,
        "the first tab draws its own group only"
    );
}

fn project_with(effect_type: &str, model: &str) -> project::project::Project {
    use domain::ids::{BlockId, ChainId};
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};

    project::project::Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![project::chain::Chain {
            id: ChainId("test:chain".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![AudioBlock {
                id: BlockId("b-1".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: effect_type.to_string(),
                    model: model.to_string(),
                    params: ParameterSet::default(),
                }),
            }],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }
}

fn compact_cells(rows: &[crate::CompactBlockItem]) -> usize {
    use slint::Model;
    rows[0]
        .parameter_lines
        .iter()
        .map(|line| line.cells.row_count())
        .sum()
}

/// The compact chain row reads the same rule, so a block drawn whole in the
/// editor is drawn whole there too — it used to derive its own groups.
#[test]
fn a_compact_row_of_a_native_amp_draws_every_knob() {
    use slint::Model;

    let rows = crate::compact_block_view::build_compact_blocks(
        &project_with("amp", "blackface_clean"),
        0,
        &[],
    );
    assert_eq!(
        rows[0].parameter_groups.row_count(),
        0,
        "the amp's compact row shows no tab bar either"
    );
    assert_eq!(compact_cells(&rows), 10, "every parameter is on the strip");
}

/// And a block that DOES have tabs shows one group at a time in the strip,
/// exactly as the editor does.
#[test]
fn a_compact_row_of_a_tabbed_block_draws_the_active_group_only() {
    use slint::Model;

    let rows = crate::compact_block_view::build_compact_blocks(
        &project_with("wah", "cry_classic"),
        0,
        &[],
    );
    let groups: Vec<String> = rows[0]
        .parameter_groups
        .iter()
        .map(|g| g.to_string())
        .collect();
    assert_eq!(groups, vec!["Wah", "Output"]);
    assert_eq!(
        compact_cells(&rows),
        2,
        "the strip draws the active tab's parameters only"
    );
}
