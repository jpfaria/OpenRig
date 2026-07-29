//! #829: the tuner readings every transport reads are built from the same
//! row model the GUI renders — no parallel derivation.

use super::{readings_from, RowIdentity};
use crate::TunerRow;
use slint::{SharedString, VecModel};

fn row(label: &str, note: &str, octave: i32, cents: f32, frequency: f32, active: bool) -> TunerRow {
    TunerRow {
        label: label.into(),
        note: SharedString::from(note),
        octave,
        cents,
        frequency,
        active,
    }
}

#[test]
fn readings_pair_each_row_with_its_chain_input_and_channel() {
    let identities = vec![
        RowIdentity {
            chain: "rig:input-1".to_string(),
            input: 0,
            channel: 0,
        },
        RowIdentity {
            chain: "rig:input-2".to_string(),
            input: 1,
            channel: 3,
        },
    ];
    let rows = VecModel::from(vec![
        row("GUITAR  ·  IN 1  ·  CH 1", "E", 2, -4.5, 82.1, true),
        row("BASS  ·  IN 2  ·  CH 4", "—", 0, 0.0, 0.0, false),
    ]);

    let readings = readings_from(&identities, &rows);

    assert_eq!(readings.len(), 2);
    assert_eq!(readings[0].chain, "rig:input-1");
    assert_eq!(readings[0].channel, 0);
    assert_eq!(readings[0].note, "E");
    assert_eq!(readings[0].octave, 2);
    assert_eq!(readings[0].cents, -4.5);
    assert_eq!(readings[0].frequency, 82.1);
    assert!(readings[0].active);
    assert_eq!(readings[1].chain, "rig:input-2");
    assert_eq!(readings[1].input, 1);
    assert_eq!(readings[1].channel, 3);
    assert!(!readings[1].active);
}

#[test]
fn readings_are_empty_when_no_rows_exist() {
    let rows = VecModel::from(Vec::<TunerRow>::new());
    assert!(readings_from(&[], &rows).is_empty());
}
