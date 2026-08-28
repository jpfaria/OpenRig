//! #915 — a tab bar is worth its 40px only when the block does NOT fit the
//! panel. A native amp has ten parameters: they wrap into two rows of the
//! grid, so splitting them across seven tabs (five of them holding a single
//! knob) hides an amp's own front panel behind a filing cabinet. The tabs stay
//! for what they were built for — a plugin with more parameters than the panel
//! can show at once.

use crate::block_editor::{block_parameter_items_for_model, parameter_groups};
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
fn a_native_amp_is_one_panel_with_no_tabs() {
    let param_items = items("amp", "blackface_clean");
    assert!(
        parameter_groups(&param_items).len() > 1,
        "the schema still groups the parameters — the grouping is what we choose not to render as tabs"
    );
    let (groups, rows) = groups_and_rows(&param_items);
    assert!(
        groups.is_empty(),
        "ten knobs fit the panel, so the amp shows no tab bar; got {groups:?}"
    );
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
fn every_native_block_that_fits_the_panel_drops_its_tabs() {
    for (effect_type, model) in [
        ("amp", "chime"),
        ("amp", "tweed_breakup"),
        ("preamp", "american_clean"),
        ("preamp", "brit_crunch"),
        ("preamp", "modern_high_gain"),
        ("cab", "brit_4x12"),
        ("gain", "tube_saturation"),
        ("gain", "fuzz_ge"),
        ("wah", "cry_classic"),
    ] {
        let param_items = items(effect_type, model);
        let (groups, rows) = groups_and_rows(&param_items);
        assert!(
            groups.is_empty(),
            "{effect_type}/{model} has {} parameters — they fit one panel, so no tab bar; got {groups:?}",
            param_items.len()
        );
        assert_eq!(
            drawn(&rows).len(),
            param_items.len(),
            "{effect_type}/{model} draws every parameter at once"
        );
    }
}

fn synthetic(count: usize, groups: usize) -> Vec<BlockParameterItem> {
    (0..count)
        .map(|i| BlockParameterItem {
            path: format!("p{i}").into(),
            label: format!("P{i}").into(),
            group: format!("G{}", i % groups).into(),
            widget_kind: "knob".into(),
            ..Default::default()
        })
        .collect()
}

#[test]
fn a_plugin_too_big_for_the_panel_keeps_its_tabs() {
    // Two rows of six is what the grid shows at once; the thirteenth parameter
    // is the one that needs the tabs.
    let (groups, rows) = groups_and_rows(&synthetic(13, 3));
    assert_eq!(groups, vec!["G0", "G1", "G2"]);
    assert_eq!(
        drawn(&rows).len(),
        5,
        "the first tab draws its own group only"
    );

    let (groups, _) = groups_and_rows(&synthetic(12, 3));
    assert!(
        groups.is_empty(),
        "twelve parameters still fit the panel; got {groups:?}"
    );
}

/// The compact chain row reads the same rule: a block that fits one panel is
/// drawn whole there too, so its row never grows a tab bar the editor does not
/// have.
#[test]
fn a_compact_row_of_a_native_amp_has_no_tab_bar() {
    use domain::ids::{BlockId, ChainId};
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use slint::Model;

    let block = AudioBlock {
        id: BlockId("amp-1".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".to_string(),
            model: "blackface_clean".to_string(),
            params: ParameterSet::default(),
        }),
    };
    let project = project::project::Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![project::chain::Chain {
            id: ChainId("test:chain".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![block],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };

    let rows = crate::compact_block_view::build_compact_blocks(&project, 0, &[]);
    assert_eq!(
        rows[0].parameter_groups.row_count(),
        0,
        "the amp fits one panel — its compact row shows no tab bar"
    );
    let drawn: usize = rows[0]
        .parameter_lines
        .iter()
        .map(|line| line.cells.row_count())
        .sum();
    assert_eq!(drawn, 10, "every parameter is on the strip");
}
