//! #928 — every meter row names its E/S. The row payload carries the input
//! and output binding names next to the readings, so the screen can print
//! "GUITARRA 1 - MAIN" instead of a bare "INPUT" / "OUTPUT".

use engine::stream_io_labels::StreamIoLabels;

use crate::meter_taps::StreamMeterReading;
use crate::meter_wiring::rebuild_stream_meters_row;

fn labels() -> Vec<StreamIoLabels> {
    vec![
        StreamIoLabels {
            input: "GUITARRA 1 - MAIN".into(),
            output: "GUITARRA 1 - MAIN".into(),
        },
        StreamIoLabels {
            input: "GUITARRA 1 - SYN5050".into(),
            output: "GUITARRA 1 - SYN5050".into(),
        },
    ]
}

#[test]
fn every_row_carries_the_names_of_its_bindings() {
    let readings = vec![
        StreamMeterReading {
            in_dbfs: -20.0,
            out_dbfs: -18.0,
        },
        StreamMeterReading {
            in_dbfs: -30.0,
            out_dbfs: -28.0,
        },
    ];

    let rows = rebuild_stream_meters_row(&readings, 2, &labels(), 100.0, true);

    let names: Vec<(String, String)> = rows
        .iter()
        .map(|r| (r.in_label.to_string(), r.out_label.to_string()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("GUITARRA 1 - MAIN".to_string(), "GUITARRA 1 - MAIN".to_string()),
            ("GUITARRA 1 - SYN5050".to_string(), "GUITARRA 1 - SYN5050".to_string()),
        ],
        "#928: the row names the E/S it reads from and writes to"
    );
}

#[test]
fn a_row_the_engine_has_not_filled_yet_is_still_named() {
    let rows = rebuild_stream_meters_row(&[], 2, &labels(), 100.0, true);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].in_label.to_string(), "GUITARRA 1 - SYN5050");
    assert_eq!(rows[1].out_label.to_string(), "GUITARRA 1 - SYN5050");
}
