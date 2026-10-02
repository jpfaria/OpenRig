//! Responsibility: decides which category tabs the player library shows.
//!
//! The four parts a player picks from are always offered, even empty, so the
//! tabs never jump around; `Other` shows only when some track has no part.

use application::player_library::BackingTrack;
use application::player_track_category::TrackCategory;

const PARTS: [TrackCategory; 4] = [
    TrackCategory::Solo,
    TrackCategory::Rhythm,
    TrackCategory::Bass,
    TrackCategory::Acoustic,
];

pub(crate) fn category_tabs(tracks: &[BackingTrack]) -> Vec<TrackCategory> {
    let mut tabs = PARTS.to_vec();
    if tracks.iter().any(|t| t.category == TrackCategory::Other) {
        tabs.push(TrackCategory::Other);
    }
    tabs
}

pub(crate) fn tracks_in(
    tracks: &[BackingTrack],
    category: TrackCategory,
) -> impl Iterator<Item = &BackingTrack> {
    tracks.iter().filter(move |t| t.category == category)
}

/// The tab a bridge has selected; a key it does not know means the first tab.
pub(crate) fn selected_category(key: &str) -> TrackCategory {
    TrackCategory::from_key(key).unwrap_or(TrackCategory::Solo)
}

#[cfg(test)]
#[path = "player_category_view_tests.rs"]
mod tests;
