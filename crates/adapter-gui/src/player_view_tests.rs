use super::*;
use std::path::Path;

#[test]
fn clock_is_minutes_and_two_digit_seconds() {
    assert_eq!(format_clock(0.0), "0:00");
    assert_eq!(format_clock(5.9), "0:05");
    assert_eq!(format_clock(83.2), "1:23");
    assert_eq!(format_clock(3665.0), "61:05");
}

#[test]
fn clock_never_shows_a_negative_or_broken_time() {
    assert_eq!(format_clock(-3.0), "0:00");
    assert_eq!(format_clock(f64::NAN), "0:00");
}

#[test]
fn time_label_joins_position_and_duration() {
    assert_eq!(time_label(83.0, 296.0), "1:23 / 4:56");
}

#[test]
fn speed_label_has_two_decimals() {
    assert_eq!(speed_label(0.75), "0.75×");
    assert_eq!(speed_label(1.0), "1.00×");
    assert_eq!(speed_label(2.0), "2.00×");
}

#[test]
fn semitones_label_is_signed() {
    assert_eq!(semitones_label(2.0), "+2 ST");
    assert_eq!(semitones_label(-2.0), "-2 ST");
    assert_eq!(semitones_label(0.0), "0 ST");
    assert_eq!(semitones_label(-0.2), "0 ST");
}

#[test]
fn loop_label_shows_the_range_or_the_pending_start() {
    assert_eq!(loop_label(Some((60.0, 120.0)), None), "1:00 – 2:00");
    assert_eq!(loop_label(None, Some(60.0)), "1:00 –");
    assert_eq!(loop_label(None, None), "");
}

#[test]
fn a_pending_start_wins_over_the_old_loop() {
    assert_eq!(loop_label(Some((60.0, 120.0)), Some(10.0)), "0:10 –");
}

#[test]
fn track_name_is_the_file_stem() {
    assert_eq!(
        track_name(Path::new("/a/Slow Blues in A.m4a")),
        "Slow Blues in A"
    );
    assert_eq!(track_name(Path::new("jam")), "jam");
}

#[test]
fn the_first_loop_press_marks_the_start() {
    assert_eq!(loop_press(None, 12.5, false), LoopPress::Mark(12.5));
}

#[test]
fn the_second_loop_press_closes_the_loop() {
    assert_eq!(
        loop_press(Some(10.0), 20.0, false),
        LoopPress::Set {
            start: 10.0,
            end: 20.0
        }
    );
}

#[test]
fn a_loop_closed_backwards_is_sorted() {
    assert_eq!(
        loop_press(Some(20.0), 10.0, false),
        LoopPress::Set {
            start: 10.0,
            end: 20.0
        }
    );
}

#[test]
fn a_second_press_too_close_marks_again() {
    assert_eq!(loop_press(Some(10.0), 10.1, false), LoopPress::Mark(10.1));
}

#[test]
fn a_press_on_a_set_loop_turns_it_off() {
    assert_eq!(loop_press(None, 30.0, true), LoopPress::Clear);
}

#[test]
fn a_pending_start_closes_even_over_an_old_loop() {
    assert_eq!(
        loop_press(Some(10.0), 20.0, true),
        LoopPress::Set {
            start: 10.0,
            end: 20.0
        }
    );
}
