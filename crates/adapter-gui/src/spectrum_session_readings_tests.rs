//! #829: the spectrum readings every transport reads are built from the
//! same row model the GUI renders — no parallel derivation.

use super::{readings_from, RowIdentity};
use crate::SpectrumRow;
use slint::{ModelRc, VecModel};
use std::rc::Rc;

fn row(label: &str, levels: Vec<f32>, peaks: Vec<f32>, active: bool) -> SpectrumRow {
    SpectrumRow {
        label: label.into(),
        levels: ModelRc::from(Rc::new(VecModel::from(levels))),
        peaks: ModelRc::from(Rc::new(VecModel::from(peaks))),
        active,
    }
}

#[test]
fn readings_carry_band_levels_and_peaks_per_tap() {
    let identities = vec![
        RowIdentity {
            chain: "rig:input-1".to_string(),
            input: 0,
            channel: 0,
        },
        RowIdentity {
            chain: "rig:input-1".to_string(),
            input: 0,
            channel: 1,
        },
    ];
    let rows = VecModel::from(vec![
        row("GUITAR · L", vec![0.1, 0.4], vec![0.2, 0.5], true),
        row("GUITAR · R", vec![0.0, 0.0], vec![0.0, 0.0], false),
    ]);

    let readings = readings_from(&identities, &rows);

    assert_eq!(readings.len(), 2);
    assert_eq!(readings[0].chain, "rig:input-1");
    assert_eq!(readings[0].channel, 0);
    assert_eq!(readings[0].levels, vec![0.1, 0.4]);
    assert_eq!(readings[0].peaks, vec![0.2, 0.5]);
    assert!(readings[0].active);
    assert_eq!(readings[1].channel, 1);
    assert!(!readings[1].active);
}

#[test]
fn readings_are_empty_when_no_rows_exist() {
    let rows = VecModel::from(Vec::<SpectrumRow>::new());
    assert!(readings_from(&[], &rows).is_empty());
}
