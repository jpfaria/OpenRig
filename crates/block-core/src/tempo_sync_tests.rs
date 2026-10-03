use super::*;

#[test]
fn every_option_but_off_maps_to_a_beat_length() {
    for (value, _) in SYNC_OPTIONS {
        if *value == SYNC_OFF {
            assert_eq!(sync_beats(value), None, "off has no beat length");
        } else {
            assert!(sync_beats(value).is_some(), "{value} must map to beats");
        }
    }
}

#[test]
fn note_values_are_measured_in_quarter_notes() {
    assert_eq!(sync_beats("1/1"), Some(4.0));
    assert_eq!(sync_beats("1/2"), Some(2.0));
    assert_eq!(sync_beats("1/4"), Some(1.0));
    assert_eq!(sync_beats("1/8"), Some(0.5));
    assert_eq!(sync_beats("1/16"), Some(0.25));
}

#[test]
fn dotted_adds_half_and_triplet_takes_two_thirds() {
    assert_eq!(sync_beats("1/8d"), Some(0.75));
    let triplet = sync_beats("1/4t").expect("1/4t");
    assert!((triplet - 2.0 / 3.0).abs() < 1e-6);
}

#[test]
fn unknown_option_has_no_beat_length() {
    assert_eq!(sync_beats("1/3"), None);
    assert_eq!(sync_beats(""), None);
}

#[test]
fn a_quarter_at_120_bpm_is_500_ms() {
    assert!((synced_time_ms(120.0, 1.0) - 500.0).abs() < 1e-3);
}

#[test]
fn a_dotted_eighth_at_100_bpm_is_450_ms() {
    assert!((synced_time_ms(100.0, 0.75) - 450.0).abs() < 1e-3);
}

#[test]
fn a_quarter_at_120_bpm_is_2_hz() {
    assert!((synced_rate_hz(120.0, 1.0) - 2.0).abs() < 1e-6);
}

#[test]
fn a_whole_note_at_60_bpm_is_a_quarter_hz() {
    assert!((synced_rate_hz(60.0, 4.0) - 0.25).abs() < 1e-6);
}

#[test]
fn sync_parameters_are_selects_that_default_to_off() {
    for spec in [time_sync_parameter(), rate_sync_parameter()] {
        assert_eq!(spec.widget, crate::param::ParameterWidget::Select);
        assert_eq!(
            spec.default_value,
            Some(domain::value_objects::ParameterValue::String(
                SYNC_OFF.to_string()
            ))
        );
        let crate::param::ParameterDomain::Enum { options } = &spec.domain else {
            panic!("{} must be an enum", spec.path);
        };
        assert_eq!(options.len(), SYNC_OPTIONS.len());
    }
    assert_eq!(time_sync_parameter().path, TIME_SYNC_PATH);
    assert_eq!(rate_sync_parameter().path, RATE_SYNC_PATH);
}

#[test]
fn each_sync_path_names_the_value_it_drives() {
    assert_eq!(synced_value_path(TIME_SYNC_PATH), Some(TIME_PATH));
    assert_eq!(synced_value_path(RATE_SYNC_PATH), Some(RATE_PATH));
    assert_eq!(synced_value_path("mix"), None);
    assert_eq!(sync_path_for_value(TIME_PATH), Some(TIME_SYNC_PATH));
    assert_eq!(sync_path_for_value(RATE_PATH), Some(RATE_SYNC_PATH));
    assert_eq!(sync_path_for_value("mix"), None);
}
