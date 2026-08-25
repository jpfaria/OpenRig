//! #878 — an equalizer shows sliders, nothing else. Every parameter that
//! belongs to a band the widget draws — the band's on/off toggle, its filter
//! type — belongs to that band, so the widget owns it too. Rendering those in
//! the panel strip produced a row of loose ENABLED toggles and TYPE dropdowns
//! above the sliders, which no equalizer has. Only the parameters of a group
//! the widget does NOT draw (the output trim) stay in the grid.

use adapter_gui::block_editor_param_tabs::retag_all;
use adapter_gui::BlockParameterItem;

fn row(path: &str, group: &str, widget: &str) -> BlockParameterItem {
    let mut it = BlockParameterItem::default();
    it.path = path.into();
    it.group = group.into();
    it.widget_kind = widget.into();
    it
}

#[test]
fn band_toggles_and_types_are_left_to_the_eq_widget() {
    let items = vec![
        row("band1_enabled", "Band 1", "toggle"),
        row("band1_type", "Band 1", "select"),
        row("band1_freq", "Band 1", ""),
        row("band1_gain", "Band 1", ""),
        row("band2_enabled", "Band 2", "toggle"),
        row("band2_type", "Band 2", "select"),
        row("band2_freq", "Band 2", ""),
        row("output_db", "Output", "knob"),
    ];

    let rendered: Vec<String> = retag_all(&items)
        .iter()
        .filter(|it| it.tab_slot >= 0)
        .map(|it| it.path.to_string())
        .collect();

    assert_eq!(
        rendered,
        vec!["output_db".to_string()],
        "the panel must render only the groups the EQ widget does not draw"
    );
}
