use super::*;
use serde_json::Value;

fn tuner_row() -> TunerReading {
    TunerReading {
        chain: "rig:input-1".to_string(),
        input: 0,
        channel: 1,
        label: "GUITAR  ·  IN 1  ·  CH 2".to_string(),
        note: "E".to_string(),
        octave: 2,
        cents: -4.5,
        frequency: 82.1,
        active: true,
    }
}

#[test]
fn tuner_readings_json_carries_note_cents_and_frequency_per_row() {
    let json: Value =
        serde_json::from_str(&tuner_readings_json(true, 440.0, &[tuner_row()])).unwrap();

    assert_eq!(json["running"], true);
    assert_eq!(json["reference_hz"], 440.0);
    let row = &json["rows"][0];
    assert_eq!(row["chain"], "rig:input-1");
    assert_eq!(row["input"], 0);
    assert_eq!(row["channel"], 1);
    assert_eq!(row["note"], "E");
    assert_eq!(row["octave"], 2);
    assert_eq!(row["cents"], -4.5);
    assert_eq!(row["frequency"], 82.1);
    assert_eq!(row["active"], true);
}

#[test]
fn tuner_readings_json_reports_powered_off_with_no_rows() {
    let json: Value = serde_json::from_str(&tuner_readings_json(false, 440.0, &[])).unwrap();

    assert_eq!(json["running"], false);
    assert_eq!(json["rows"].as_array().unwrap().len(), 0);
}

#[test]
fn spectrum_readings_json_carries_band_frequencies_once_and_levels_per_row() {
    let row = SpectrumReading {
        chain: "rig:input-1".to_string(),
        input: 0,
        channel: 0,
        label: "GUITAR  ·  IN 1".to_string(),
        levels: vec![0.1, 0.5, 0.9],
        peaks: vec![0.2, 0.6, 1.0],
        active: true,
    };

    let json: Value =
        serde_json::from_str(&spectrum_readings_json(true, &[20.0, 22.4, 25.2], &[row])).unwrap();

    assert_eq!(json["running"], true);
    assert_eq!(json["band_hz"][1], 22.4);
    assert_eq!(json["rows"][0]["levels"][2], 0.9);
    assert_eq!(json["rows"][0]["peaks"][0], 0.2);
    assert_eq!(json["rows"][0]["chain"], "rig:input-1");
    assert_eq!(json["rows"][0]["active"], true);
}
