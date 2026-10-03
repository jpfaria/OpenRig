use slint::{Global, Model};

use feature_dsp::drums::DrumPosition;

use super::{set_drums_position, set_drums_view};
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

#[test]
fn the_scalar_state_reaches_the_bridge() {
    i_slint_backend_testing::init_no_event_loop();
    let w = DrumsWindow::new().unwrap();
    let bridge = DrumsBridge::get(&w);
    let v = DrumsView {
        enabled: true,
        playing: true,
        bpm: 133.0,
        volume: 0.5,
        beats_per_bar: 3,
        kit_key: "red".into(),
        kit_label: "Red Zeppelin".into(),
        ..view()
    };
    set_drums_view(&bridge, &v);
    assert!(bridge.get_enabled() && bridge.get_playing());
    assert_eq!(bridge.get_bpm(), 133.0);
    assert_eq!(bridge.get_volume(), 0.5);
    assert_eq!(bridge.get_beats_per_bar(), 3);
    assert_eq!(bridge.get_kit_key(), "red");
    assert_eq!(bridge.get_kit_label(), "Red Zeppelin");
}

#[test]
fn the_lamps_follow_the_position_and_no_runtime_reads_as_stopped() {
    i_slint_backend_testing::init_no_event_loop();
    let w = DrumsWindow::new().unwrap();
    let bridge = DrumsBridge::get(&w);
    set_drums_position(
        &bridge,
        Some(DrumPosition {
            playing: true,
            bar: 2,
            beat: 3,
            in_fill: true,
        }),
    );
    assert_eq!(
        (bridge.get_current_bar(), bridge.get_current_beat()),
        (2, 3)
    );
    assert!(bridge.get_in_fill());

    set_drums_position(&bridge, None);
    assert_eq!(
        (bridge.get_current_bar(), bridge.get_current_beat()),
        (0, 0)
    );
    assert!(!bridge.get_in_fill());
}

#[test]
fn a_fill_only_lights_while_the_groove_plays() {
    i_slint_backend_testing::init_no_event_loop();
    let w = DrumsWindow::new().unwrap();
    let bridge = DrumsBridge::get(&w);
    set_drums_position(
        &bridge,
        Some(DrumPosition {
            playing: false,
            bar: 0,
            beat: 0,
            in_fill: true,
        }),
    );
    assert!(!bridge.get_in_fill());
}
