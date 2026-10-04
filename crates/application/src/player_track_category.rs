//! Responsibility: names the part a backing track leaves for the player.
//!
//! A track's category is the folder it sits in: `solo`, `rhythm`, `bass` or
//! `acoustic`, in the bundled library and in the user's own folder alike.
//! Anything else — a file at the top of a folder, an unknown subfolder — is
//! `Other`.

/// What the player plays over the track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackCategory {
    /// A full band with rhythm guitar: the player takes the lead.
    Solo,
    /// Drums and bass, no guitar: the player takes the rhythm part.
    Rhythm,
    /// Everything but the bass.
    Bass,
    /// An acoustic arrangement to strum over.
    Acoustic,
    Other,
}

impl TrackCategory {
    pub const ALL: [TrackCategory; 5] = [
        TrackCategory::Solo,
        TrackCategory::Rhythm,
        TrackCategory::Bass,
        TrackCategory::Acoustic,
        TrackCategory::Other,
    ];

    /// The folder name and the panel's tab key.
    pub fn key(self) -> &'static str {
        match self {
            TrackCategory::Solo => "solo",
            TrackCategory::Rhythm => "rhythm",
            TrackCategory::Bass => "bass",
            TrackCategory::Acoustic => "acoustic",
            TrackCategory::Other => "other",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|category| category.key() == key)
    }

    /// The category a folder of that name holds, ignoring case.
    pub fn from_folder(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|category| category.key().eq_ignore_ascii_case(name))
            .filter(|category| *category != TrackCategory::Other)
            .unwrap_or(TrackCategory::Other)
    }
}

#[cfg(test)]
#[path = "player_track_category_tests.rs"]
mod tests;
