use super::*;

#[test]
fn a_folder_name_maps_to_its_category_in_any_case() {
    assert_eq!(TrackCategory::from_folder("solo"), TrackCategory::Solo);
    assert_eq!(TrackCategory::from_folder("Rhythm"), TrackCategory::Rhythm);
    assert_eq!(TrackCategory::from_folder("BASS"), TrackCategory::Bass);
    assert_eq!(
        TrackCategory::from_folder("acoustic"),
        TrackCategory::Acoustic
    );
    assert_eq!(TrackCategory::from_folder("misc"), TrackCategory::Other);
}

#[test]
fn every_category_round_trips_through_its_key() {
    for category in TrackCategory::ALL {
        assert_eq!(TrackCategory::from_key(category.key()), Some(category));
    }
    assert_eq!(TrackCategory::from_key("nope"), None);
}
