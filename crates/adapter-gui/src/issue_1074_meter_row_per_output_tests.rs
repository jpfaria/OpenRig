//! #1074 — one jack is one pipeline feeding every output, so a stream's meter
//! row splits into one row per output: the input is shown once, and each
//! output shows the level its own route played (after its faders).

use crate::meter_taps::StreamMeterReading;
use engine::output_meter::SILENT_DBFS;
use engine::stream_io_labels::{OutputIoLabel, StreamIoLabels};

fn out(route: usize, name: &str, channels: &str) -> OutputIoLabel {
    OutputIoLabel {
        route,
        name: name.into(),
        channels: channels.into(),
        device: String::new(),
    }
}

fn labels() -> Vec<StreamIoLabels> {
    vec![StreamIoLabels {
        input: "GUITARRA 1".into(),
        output: "MAIN + FRFR + SYN-5050".into(),
        input_channels: "1".into(),
        output_channels: "1,2 + 25,26 + 5,6".into(),
        outputs: vec![
            out(0, "MAIN", "1,2"),
            out(1, "FRFR", "25,26"),
            out(2, "SYN-5050", "5,6"),
        ],
        ..Default::default()
    }]
}

fn reading() -> Vec<StreamMeterReading> {
    vec![StreamMeterReading {
        in_dbfs: -20.0,
        out_dbfs: -6.0,
        route_out_dbfs: vec![-6.0, -12.0, -30.0],
    }]
}

#[test]
fn one_stream_with_three_outputs_is_three_rows_with_the_input_once() {
    let rows =
        crate::meter_wiring::rebuild_stream_meters_row(&reading(), 1, &labels(), 100.0, true);

    let outs: Vec<(String, String, f32)> = rows
        .iter()
        .map(|r| {
            (
                r.out_label.to_string(),
                r.out_channels.to_string(),
                r.out_dbfs,
            )
        })
        .collect();
    assert_eq!(
        outs,
        vec![
            ("MAIN".to_string(), "1,2".to_string(), -6.0),
            ("FRFR".to_string(), "25,26".to_string(), -12.0),
            ("SYN-5050".to_string(), "5,6".to_string(), -30.0),
        ]
    );
    let repeated: Vec<bool> = rows.iter().map(|r| r.in_repeated).collect();
    assert_eq!(
        repeated,
        vec![false, true, true],
        "the input is listed once"
    );
    assert!(rows.iter().all(|r| r.in_dbfs == -20.0));
}

#[test]
fn a_route_level_already_carries_the_chain_volume() {
    let rows = crate::meter_wiring::rebuild_stream_meters_row(&reading(), 1, &labels(), 50.0, true);

    let outs: Vec<f32> = rows.iter().map(|r| r.out_dbfs).collect();
    assert_eq!(outs, vec![-6.0, -12.0, -30.0]);
}

#[test]
fn before_the_engine_reports_the_rows_are_already_one_per_output() {
    let rows = crate::meter_wiring::rebuild_stream_meters_row(&[], 1, &labels(), 100.0, true);

    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|r| r.out_dbfs == SILENT_DBFS));
    assert_eq!(rows[2].out_label.to_string(), "SYN-5050");
}
