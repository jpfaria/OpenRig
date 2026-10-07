use std::collections::BTreeSet;

use slint::{Global, Model};

use super::{set_plugins_view, type_options, PluginsChrome};
use crate::plugins_view::{PluginRowView, PluginsView, TypeOption};
use crate::{PluginsBridge, PluginsWindow};

fn row(id: &str, tone3000: bool) -> PluginRowView {
    PluginRowView {
        plugin_id: id.into(),
        name: "Plexi".into(),
        brand: "Marshall".into(),
        effect_type: "amp".into(),
        type_label: "Amp".into(),
        backend: "NAM".into(),
        arch: "A2".into(),
        captures: 7,
        tone3000,
        editable: true,
    }
}

fn types() -> Vec<TypeOption> {
    vec![
        TypeOption {
            key: "amp".into(),
            label: "Amp".into(),
        },
        TypeOption {
            key: "cab".into(),
            label: "Cab".into(),
        },
    ]
}

#[test]
fn the_rows_count_and_error_reach_the_window() {
    i_slint_backend_testing::init_no_event_loop();
    let w = PluginsWindow::new().unwrap();
    let view = PluginsView {
        rows: vec![row("plexi", true), row("v30", false)],
        total: 5,
        types: types(),
    };
    let busy = BTreeSet::from(["plexi".to_string()]);
    let chrome = PluginsChrome {
        busy: &busy,
        error: "disk full",
        type_query: "",
    };
    set_plugins_view(&PluginsBridge::get(&w), &view, &chrome);
    let bridge = PluginsBridge::get(&w);
    let rows = bridge.get_rows();
    assert_eq!(rows.row_count(), 2);
    let first = rows.row_data(0).unwrap();
    assert_eq!(first.plugin_id, "plexi");
    assert!(first.tone3000 && first.busy && first.editable);
    assert_eq!(first.captures, 7);
    assert!(!rows.row_data(1).unwrap().busy);
    assert_eq!(bridge.get_total(), 5);
    assert_eq!(bridge.get_error(), "disk full");
    assert_eq!(bridge.get_types().row_count(), 3);
}

#[test]
fn every_type_comes_first_then_the_matching_types() {
    let options = type_options("All types", &types(), " CA ");
    let keys: Vec<&str> = options.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, vec!["", "cab"]);
    assert_eq!(options[0].label, "All types");
    assert_eq!(type_options("All", &types(), "").len(), 3);
}
