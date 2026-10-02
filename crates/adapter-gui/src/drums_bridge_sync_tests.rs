use slint::{Global, Model};

use super::set_drums_view;
use crate::drums_view::{DrumPick, DrumsView};
use crate::{DrumsBridge, DrumsWindow};

fn pick(key: &str, label: &str, header: bool) -> DrumPick {
    DrumPick {
        key: key.into(),
        label: label.into(),
        header,
    }
}

fn view() -> DrumsView {
    DrumsView {
        kits: vec![
            pick("black-pearl", "Black Pearl", false),
            pick("red", "Red Zeppelin", false),
        ],
        grooves: vec![
            pick("", "rock", true),
            pick("rock-01", "Rock 1", false),
            pick("", "jazz", true),
            pick("jazz-01", "Jazz 1", false),
        ],
        outputs: vec![
            pick("main\u{1f}MAIN", "MAIN + FRFR · MAIN", false),
            pick("syn\u{1f}SYN", "SYN5050 · SYN-5050", false),
        ],
        ..DrumsView::default()
    }
}

fn labels(rows: slint::ModelRc<crate::DrumPickRow>) -> Vec<String> {
    rows.iter().map(|r| r.label.to_string()).collect()
}

#[test]
fn the_rows_follow_the_bridge_search() {
    i_slint_backend_testing::init_no_event_loop();
    let w = DrumsWindow::new().unwrap();
    let bridge = DrumsBridge::get(&w);
    bridge.set_query("jazz".into());
    set_drums_view(&bridge, &view());
    assert_eq!(labels(bridge.get_groove_rows()), vec!["jazz", "Jazz 1"]);
    assert!(labels(bridge.get_kit_rows()).is_empty());

    bridge.set_query("".into());
    set_drums_view(&bridge, &view());
    assert_eq!(labels(bridge.get_kit_rows()).len(), 2);
    assert_eq!(labels(bridge.get_output_rows()).len(), 2);
}
