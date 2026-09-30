//! #1007: the fader law — where a gain sits on the fader and back.

use super::*;

#[test]
fn the_ends_of_the_fader_are_the_ends_of_the_range() {
    assert_eq!(db_from_position(0.0), -60.0);
    assert_eq!(db_from_position(1.0), 12.0);
    assert_eq!(position_from_db(-60.0), 0.0);
    assert_eq!(position_from_db(12.0), 1.0);
}

#[test]
fn unity_sits_high_on_the_fader_like_a_console() {
    let unity = position_from_db(0.0);
    assert!((0.7..0.85).contains(&unity), "unity at {unity}");
}

#[test]
fn the_law_round_trips() {
    for db in [-60.0, -45.0, -30.0, -12.0, -6.0, 0.0, 3.0, 12.0] {
        let back = db_from_position(position_from_db(db));
        assert!((back - db).abs() < 1e-3, "{db} came back as {back}");
    }
}

#[test]
fn out_of_range_input_is_clamped() {
    assert_eq!(db_from_position(-1.0), -60.0);
    assert_eq!(db_from_position(2.0), 12.0);
    assert_eq!(position_from_db(-200.0), 0.0);
    assert_eq!(position_from_db(40.0), 1.0);
}

#[test]
fn the_lower_half_gets_more_travel_per_db_than_a_straight_line() {
    // A linear law would put -30 dB at 30/72 of the way up; a console law
    // spends less of the travel on the quiet end.
    assert!(position_from_db(-30.0) < 30.0 / 72.0);
}

#[test]
fn a_drag_lands_on_a_tenth_of_a_db() {
    let db = db_from_position(0.5);
    assert_eq!(db, (db * 10.0).round() / 10.0);
}

#[test]
fn the_gain_label_reads_like_a_console() {
    assert_eq!(gain_label(0.0), "0.0 dB");
    assert_eq!(gain_label(-6.0), "-6.0 dB");
    assert_eq!(gain_label(3.5), "+3.5 dB");
    assert_eq!(gain_label(-60.0), "-60.0 dB");
}
