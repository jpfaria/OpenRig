//! #553 — track catalog lifecycle + transport + per-stem controls.
//!
//! Same precedent as `local_dispatcher_stems`: the dispatcher records
//! the intent and emits a typed event. The adapter listens, persists
//! to `meta.yaml` / loads buffers / forwards atomics to the engine
//! `MultiStemPlayer`.

use anyhow::Result;

use crate::command::Command;
use crate::event::{Event, StemControl};
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Command::LoadTrack` / `UnloadTrack` / `RenameTrack` /
    /// `DeleteTrack`.
    pub(crate) fn handle_track_lifecycle(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::LoadTrack { track_id } => Ok(vec![Event::TrackLoadRequested { track_id }]),
            Command::UnloadTrack => Ok(vec![Event::TrackUnloaded]),
            Command::RenameTrack {
                track_id,
                new_title,
            } => Ok(vec![Event::TrackRenamed {
                track_id,
                new_title,
            }]),
            Command::DeleteTrack { track_id } => Ok(vec![Event::TrackDeleted { track_id }]),
            other => unreachable!("handle_track_lifecycle: {other:?}"),
        }
    }

    /// `Command::TrackPlay` / `TrackPause` / `TrackSeek`.
    pub(crate) fn handle_track_transport(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::TrackPlay => Ok(vec![Event::TrackPlayRequested]),
            Command::TrackPause => Ok(vec![Event::TrackPauseRequested]),
            Command::TrackSeek { secs } => Ok(vec![Event::TrackSeekRequested { secs }]),
            other => unreachable!("handle_track_transport: {other:?}"),
        }
    }

    /// `Command::SetStemMute` / `SetStemSolo` / `SetStemGain` /
    /// `SetStemPan`.
    pub(crate) fn handle_stem_controls(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SetStemMute { stem_index, muted } => Ok(vec![Event::StemControlChanged {
                stem_index,
                kind: StemControl::Mute(muted),
            }]),
            Command::SetStemSolo { stem_index, soloed } => Ok(vec![Event::StemControlChanged {
                stem_index,
                kind: StemControl::Solo(soloed),
            }]),
            Command::SetStemGain { stem_index, gain } => Ok(vec![Event::StemControlChanged {
                stem_index,
                kind: StemControl::Gain(gain),
            }]),
            Command::SetStemPan { stem_index, pan } => Ok(vec![Event::StemControlChanged {
                stem_index,
                kind: StemControl::Pan(pan),
            }]),
            other => unreachable!("handle_stem_controls: {other:?}"),
        }
    }
}
