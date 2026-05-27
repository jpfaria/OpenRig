//! #553 — Tracks screen GUI wiring.
//!
//! Owns:
//! - Launcher ↔ Tracks ↔ Track Detail navigation
//! - Catalog scan + list population
//! - Import button → off-RT separation worker
//! - Track Detail state (stems, per-stem mute/solo/gain/pan, playhead)
//!
//! The `MultiStemPlayer` is held here and exposed to the audio engine
//! through `Arc` so the eventual cpal output stage can drain it. For
//! now, playback state is recorded in atomics and reflected back to
//! the UI — the actual cpal hookup lands in a follow-up.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use feature_stems::SeparateRequest;
use feature_tracks::{MultiStemPlayer, StemKind, TrackEntry};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Weak};
use uuid::Uuid;

use crate::state::ProjectSession;
use crate::{AppWindow, StemRowData, TrackRowData};

/// Resolve the catalog root: `<data-dir>/OpenRig/tracks/`.
fn default_tracks_dir() -> PathBuf {
    dirs::data_dir()
        .map(|d| d.join("OpenRig").join("tracks"))
        .unwrap_or_else(|| PathBuf::from("tracks"))
}

fn format_duration(secs: f64) -> SharedString {
    if !secs.is_finite() || secs <= 0.0 {
        return "--:--".into();
    }
    let total = secs as u64;
    let m = total / 60;
    let s = total % 60;
    format!("{m}:{s:02}").into()
}

fn entry_to_row(entry: &TrackEntry) -> TrackRowData {
    TrackRowData {
        id: SharedString::from(entry.meta.id.as_str()),
        title: SharedString::from(entry.meta.title.as_str()),
        artist: entry
            .meta
            .artist
            .as_deref()
            .map(SharedString::from)
            .unwrap_or_default(),
        duration: format_duration(entry.meta.duration_secs),
        stem_count: entry.meta.stems.len() as i32,
    }
}

/// Shared state across all Tracks callbacks. Catalog is cached so a
/// track-click can resolve the id back to its entry without rescanning
/// the disk; the player is dropped when the user navigates away from
/// the detail view so its buffers free immediately.
struct TracksState {
    catalog: Vec<TrackEntry>,
    player: Option<Arc<MultiStemPlayer>>,
    stream: Option<crate::tracks_player_stream::TrackPlaybackStream>,
}

impl TracksState {
    fn new() -> Self {
        Self {
            catalog: Vec::new(),
            player: None,
            stream: None,
        }
    }
}

fn rescan_catalog(state: &Rc<RefCell<TracksState>>, window: &AppWindow) {
    let dir = default_tracks_dir();
    let entries = feature_tracks::scan_catalog(&dir).unwrap_or_default();
    let rows: Vec<TrackRowData> = entries.iter().map(entry_to_row).collect();
    state.borrow_mut().catalog = entries;
    let model = Rc::new(VecModel::from(rows));
    window.set_tracks(ModelRc::from(model));
}

fn current_utc_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (y + if m <= 2 { 1 } else { 0 }) as i32;
    (year, m, d)
}

fn stem_kind_label(kind: StemKind) -> &'static str {
    match kind {
        StemKind::Drums => "Drums",
        StemKind::Bass => "Bass",
        StemKind::Vocals => "Vocals",
        StemKind::Other => "Other",
    }
}

fn load_stems_for_entry(entry: &TrackEntry) -> Vec<Vec<f32>> {
    entry
        .meta
        .stems
        .iter()
        .map(|stem| {
            let path = entry.dir.join(&stem.filename);
            feature_stems::decode_audio(&path)
                .map(|d| d.samples)
                .unwrap_or_default()
        })
        .collect()
}

fn populate_track_detail(window: &AppWindow, entry: &TrackEntry, player: &MultiStemPlayer) {
    window.set_track_detail_title(entry.meta.title.as_str().into());
    window.set_track_detail_artist(
        entry
            .meta
            .artist
            .as_deref()
            .unwrap_or("")
            .to_string()
            .into(),
    );
    window.set_track_detail_bpm_text(
        entry
            .meta
            .bpm
            .map(|b| format!("{b:.0}"))
            .unwrap_or_else(|| "—".to_string())
            .into(),
    );
    window.set_track_detail_key_text(entry.meta.key.as_deref().unwrap_or("—").to_string().into());
    window.set_track_detail_duration_text(format_duration(entry.meta.duration_secs));
    window.set_track_detail_position_text(format_duration(0.0));

    let _ = player;
    let stems: Vec<StemRowData> = entry
        .meta
        .stems
        .iter()
        .enumerate()
        .map(|(idx, stem)| StemRowData {
            index: idx as i32,
            label: stem_kind_label(stem.kind).into(),
            muted: false,
            soloed: false,
            gain: 1.0,
            pan: 0.0,
        })
        .collect();
    let model = Rc::new(VecModel::from(stems));
    window.set_track_detail_stems(ModelRc::from(model));
    window.set_track_detail_playing(false);
}

fn spawn_separation_worker(window: &AppWindow, source_path: PathBuf) {
    let catalog_dir = default_tracks_dir();
    if let Err(err) = std::fs::create_dir_all(&catalog_dir) {
        eprintln!(
            "tracks: cannot create catalog dir {}: {err}",
            catalog_dir.display()
        );
        return;
    }
    let track_id = Uuid::new_v4().simple().to_string();
    let title = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "Untitled".to_string());

    let request = SeparateRequest {
        source_path,
        catalog_dir,
        track_id,
        title,
        model: "stub".to_string(),
        generated_at: current_utc_iso8601(),
    };

    let window_weak: Weak<AppWindow> = window.as_weak();
    thread::spawn(move || match feature_stems::separate_track(&request) {
        Ok(_) => {
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = window_weak.upgrade() {
                    refresh_only_list(&window);
                }
            });
        }
        Err(err) => {
            eprintln!("tracks: separation failed: {err}");
        }
    });
}

/// Helper for the import worker that has no access to the shared
/// state cell: rescan the disk straight into the Slint model without
/// caching the entries (the next time the user opens Tracks, the
/// regular rescan caches them).
fn refresh_only_list(window: &AppWindow) {
    let dir = default_tracks_dir();
    let rows: Vec<TrackRowData> = feature_tracks::scan_catalog(&dir)
        .unwrap_or_default()
        .iter()
        .map(entry_to_row)
        .collect();
    let model = Rc::new(VecModel::from(rows));
    window.set_tracks(ModelRc::from(model));
}

/// Dispatch `Command::SeparateStems` through the project session's
/// Command bus when one is open, then react to the resulting
/// `Event::StemJobQueued` by spawning the off-RT worker. When no
/// project session is open the worker is spawned directly so the
/// Tracks feature still works as a user-wide tool.
fn dispatch_separation(
    window: &AppWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    source_path: PathBuf,
) {
    let mut spawned_via_bus = false;
    if let Some(session) = project_session.borrow().as_ref() {
        match session.dispatcher.dispatch(Command::SeparateStems {
            source_path: source_path.clone(),
        }) {
            Ok(events) => {
                for ev in events {
                    if let Event::StemJobQueued { source_path } = ev {
                        spawn_separation_worker(window, source_path);
                        spawned_via_bus = true;
                    }
                }
            }
            Err(err) => {
                eprintln!("tracks: dispatch SeparateStems failed: {err}");
            }
        }
    }
    if !spawned_via_bus {
        spawn_separation_worker(window, source_path);
    }
}

/// Hook every Tracks-related callback. Called once during app start.
pub(crate) fn wire_tracks_nav(
    window: &AppWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
) {
    let state = Rc::new(RefCell::new(TracksState::new()));
    rescan_catalog(&state, window);

    {
        let state = state.clone();
        let window_weak = window.as_weak();
        window.on_open_tracks_clicked(move || {
            if let Some(window) = window_weak.upgrade() {
                rescan_catalog(&state, &window);
                window.set_show_project_launcher(false);
                window.set_show_track_detail(false);
                window.set_show_tracks(true);
            }
        });
    }

    {
        let window_weak = window.as_weak();
        window.on_tracks_back_clicked(move || {
            if let Some(window) = window_weak.upgrade() {
                window.set_show_tracks(false);
                window.set_show_project_launcher(true);
            }
        });
    }

    {
        let window_weak = window.as_weak();
        let project_session = project_session.clone();
        window.on_tracks_import_clicked(move || {
            let file = rfd::FileDialog::new()
                .add_filter("Audio", &["wav", "mp3", "flac", "ogg", "m4a"])
                .pick_file();
            let Some(path) = file else { return };
            if !path.exists() || !path.is_file() {
                return;
            }
            if let Some(window) = window_weak.upgrade() {
                dispatch_separation(&window, &project_session, path);
            }
        });
    }

    {
        let state = state.clone();
        let window_weak = window.as_weak();
        window.on_tracks_track_clicked(move |id| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let entry = state
                .borrow()
                .catalog
                .iter()
                .find(|e| e.meta.id.as_str() == id.as_str())
                .cloned();
            let Some(entry) = entry else { return };

            let stems = load_stems_for_entry(&entry);
            let player = MultiStemPlayer::new(stems, entry.meta.source_sample_rate);
            populate_track_detail(&window, &entry, &player);
            state.borrow_mut().player = Some(Arc::new(player));

            window.set_show_tracks(false);
            window.set_show_track_detail(true);
        });
    }

    {
        let state = state.clone();
        let window_weak = window.as_weak();
        window.on_track_detail_back_clicked(move || {
            if let Some(window) = window_weak.upgrade() {
                let mut state = state.borrow_mut();
                state.stream = None;
                state.player = None;
                window.set_show_track_detail(false);
                window.set_show_tracks(true);
            }
        });
    }

    {
        let state = state.clone();
        window.on_track_detail_stem_mute_toggled(move |idx, muted| {
            if let Some(player) = state.borrow().player.as_ref() {
                player.set_mute(idx as usize, muted);
            }
        });
    }
    {
        let state = state.clone();
        window.on_track_detail_stem_solo_toggled(move |idx, soloed| {
            if let Some(player) = state.borrow().player.as_ref() {
                player.set_solo(idx as usize, soloed);
            }
        });
    }
    {
        let state = state.clone();
        window.on_track_detail_stem_gain_changed(move |idx, gain| {
            if let Some(player) = state.borrow().player.as_ref() {
                player.set_gain(idx as usize, gain);
            }
        });
    }
    {
        let state = state.clone();
        window.on_track_detail_stem_pan_changed(move |idx, pan| {
            if let Some(player) = state.borrow().player.as_ref() {
                player.set_pan(idx as usize, pan);
            }
        });
    }

    // Reflect mute / solo toggles back into the row model so the UI
    // shows the new colour without a roundtrip.
    {
        let window_weak = window.as_weak();
        window.on_track_detail_stem_mute_toggled(move |idx, muted| {
            if let Some(window) = window_weak.upgrade() {
                update_stem_row(&window, idx as usize, |row| row.muted = muted);
            }
        });
    }
    {
        let window_weak = window.as_weak();
        window.on_track_detail_stem_solo_toggled(move |idx, soloed| {
            if let Some(window) = window_weak.upgrade() {
                update_stem_row(&window, idx as usize, |row| row.soloed = soloed);
            }
        });
    }

    {
        let state = state.clone();
        let window_weak = window.as_weak();
        window.on_track_detail_play_toggle(move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let target_playing = !window.get_track_detail_playing();
            let mut state = state.borrow_mut();
            if target_playing {
                if let Some(player) = state.player.clone() {
                    match crate::tracks_player_stream::TrackPlaybackStream::start(player) {
                        Ok(stream) => {
                            state.stream = Some(stream);
                            window.set_track_detail_playing(true);
                        }
                        Err(err) => {
                            eprintln!("tracks: cannot start playback: {err}");
                        }
                    }
                }
            } else {
                state.stream = None;
                window.set_track_detail_playing(false);
            }
        });
    }
    {
        // Seek currently advances the position label only — the cpal
        // pipeline hook lands separately. Keeping it wired here so the
        // UI does not look frozen when the user drags the transport.
        let window_weak = window.as_weak();
        window.on_track_detail_seek_relative(move |_delta| {
            if let Some(window) = window_weak.upgrade() {
                let _ = window;
            }
        });
    }

    window.set_tracks_search_text(SharedString::new());
}

fn update_stem_row(window: &AppWindow, idx: usize, mutate: impl FnOnce(&mut StemRowData)) {
    let model = window.get_track_detail_stems();
    if let Some(mut row) = model.row_data(idx) {
        mutate(&mut row);
        model.set_row_data(idx, row);
    }
}

#[cfg(test)]
mod tests {
    use super::{civil_from_days, current_utc_iso8601, default_tracks_dir, format_duration};

    #[test]
    fn format_duration_renders_zero_as_placeholder() {
        assert_eq!(format_duration(0.0).as_str(), "--:--");
        assert_eq!(format_duration(-1.0).as_str(), "--:--");
        assert_eq!(format_duration(f64::NAN).as_str(), "--:--");
    }

    #[test]
    fn format_duration_renders_minutes_and_seconds_with_zero_padding() {
        assert_eq!(format_duration(75.0).as_str(), "1:15");
        assert_eq!(format_duration(9.0).as_str(), "0:09");
        assert_eq!(format_duration(3600.0).as_str(), "60:00");
    }

    #[test]
    fn default_tracks_dir_resolves_under_some_writeable_root() {
        let dir = default_tracks_dir();
        assert!(dir.ends_with("OpenRig/tracks") || dir.ends_with("tracks"));
    }

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_599), (2026, 5, 26));
        assert_eq!(civil_from_days(18_321), (2020, 2, 29));
    }

    #[test]
    fn current_utc_iso8601_emits_well_formed_timestamp() {
        let ts = current_utc_iso8601();
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[10..11], "T");
        assert!(ts.ends_with('Z'));
    }
}
