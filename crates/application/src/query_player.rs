//! Responsibility: serializes the backing-track player's state for every transport.

use serde_json::json;

use crate::player_library::{list_backing_tracks, PlayerLibraryDirs};
use crate::player_state::PlayerSnapshot;

/// What the player's own stream reports right now.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlayerReading {
    /// Whether the track is audible. The end of a track turns this off on the
    /// audio side, so it wins over the dispatcher's record.
    pub playing: bool,
    pub position_seconds: f64,
    pub duration_seconds: f64,
    pub loading: bool,
    pub failed: bool,
}

/// The player as one JSON object: what the dispatcher holds, what the stream
/// reports (silent defaults when no player is hosted) and the track library.
pub fn player_state_json(
    snapshot: &PlayerSnapshot,
    live: Option<PlayerReading>,
    library: &PlayerLibraryDirs,
) -> String {
    let reading = live.unwrap_or(PlayerReading {
        playing: snapshot.playing,
        ..PlayerReading::default()
    });
    let settings = snapshot.settings;
    let value = json!({
        "track": snapshot.track.as_ref().map(|path| path.to_string_lossy()),
        "playing": reading.playing,
        "position_seconds": reading.position_seconds,
        "duration_seconds": reading.duration_seconds,
        "loading": reading.loading,
        "failed": reading.failed,
        "volume": settings.volume,
        "speed": settings.speed,
        "semitones": settings.semitones,
        "loop": settings
            .loop_range
            .map(|(start, end)| json!({ "start": start, "end": end })),
        "output": snapshot.output_key,
        "library": list_backing_tracks(library),
    });
    value.to_string()
}

#[cfg(test)]
#[path = "query_player_tests.rs"]
mod tests;
