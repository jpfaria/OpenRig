use std::path::PathBuf;

use application::player_library::BackingTrack;
use application::player_track_category::TrackCategory;

use super::*;

fn track(name: &str, category: TrackCategory) -> BackingTrack {
    BackingTrack {
        name: name.into(),
        path: PathBuf::from(format!("/t/{name}.m4a")),
        bundled: true,
        category,
    }
}

#[test]
fn the_four_parts_are_always_offered() {
    assert_eq!(
        category_tabs(&[]),
        vec![
            TrackCategory::Solo,
            TrackCategory::Rhythm,
            TrackCategory::Bass,
            TrackCategory::Acoustic
        ]
    );
}

#[test]
fn other_is_offered_only_when_a_track_has_no_part() {
    let tabs = category_tabs(&[track("Loose", TrackCategory::Other)]);
    assert_eq!(tabs.last(), Some(&TrackCategory::Other));
    assert_eq!(tabs.len(), 5);
}

#[test]
fn a_tab_lists_only_its_tracks() {
    let tracks = [
        track("A", TrackCategory::Solo),
        track("B", TrackCategory::Bass),
        track("C", TrackCategory::Solo),
    ];
    let names: Vec<_> = tracks_in(&tracks, TrackCategory::Solo)
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names, ["A", "C"]);
}

#[test]
fn an_unknown_tab_key_falls_back_to_solo() {
    assert_eq!(selected_category("bass"), TrackCategory::Bass);
    assert_eq!(selected_category(""), TrackCategory::Solo);
}
