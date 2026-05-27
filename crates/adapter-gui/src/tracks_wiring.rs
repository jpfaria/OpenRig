//! #553 — Tracks secondary-window wiring.
//!
//! Owns:
//! - Launcher / Chains → `open-tracks-clicked` → show the secondary
//!   `TracksWindow` (same pattern as Tuner/Spectrum/CompactChainView).
//! - Catalog scan + list population inside the window.
//! - Import button → off-RT separation worker (Command bus when a
//!   project session is open, direct call otherwise).
//! - Track detail state inside the window (stems, mute/solo/gain/pan,
//!   playhead) + cpal output stream.

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
use slint::{
    ComponentHandle, Image, Model, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak,
};
use uuid::Uuid;

use crate::helpers::show_child_window;
use crate::state::ProjectSession;
use crate::{AppWindow, StemRowData, TrackRowData, TracksWindow};

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

struct TracksState {
    catalog: Vec<TrackEntry>,
    player: Option<Arc<MultiStemPlayer>>,
    stream: Option<crate::tracks_player_stream::TrackPlaybackStream>,
    /// Total frames in the currently loaded track (for playhead → progress).
    total_frames: usize,
}

impl TracksState {
    fn new() -> Self {
        Self {
            catalog: Vec::new(),
            player: None,
            stream: None,
            total_frames: 0,
        }
    }
}

fn rescan_catalog(state: &Rc<RefCell<TracksState>>, tracks_window: &TracksWindow) {
    let dir = default_tracks_dir();
    let entries = feature_tracks::scan_catalog(&dir).unwrap_or_default();
    let rows: Vec<TrackRowData> = entries.iter().map(entry_to_row).collect();
    state.borrow_mut().catalog = entries;
    let model = Rc::new(VecModel::from(rows));
    tracks_window.set_tracks(ModelRc::from(model));
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
        StemKind::Guitar => "Guitar",
        StemKind::Piano => "Piano",
    }
}

fn stem_kind_icon(kind: StemKind) -> Image {
    // The instrument SVGs ship in `assets/instruments/`; map each
    // stem to the closest visual metaphor. `Other` reuses the
    // generic placeholder.
    match kind {
        StemKind::Drums => {
            Image::load_from_svg_data(include_bytes!("../../../assets/instruments/drums.svg"))
                .unwrap_or_default()
        }
        StemKind::Bass => {
            Image::load_from_svg_data(include_bytes!("../../../assets/instruments/bass.svg"))
                .unwrap_or_default()
        }
        StemKind::Vocals => {
            Image::load_from_svg_data(include_bytes!("../../../assets/instruments/voice.svg"))
                .unwrap_or_default()
        }
        StemKind::Other => {
            Image::load_from_svg_data(include_bytes!("../../../assets/instruments/generic.svg"))
                .unwrap_or_default()
        }
        StemKind::Guitar => Image::load_from_svg_data(include_bytes!(
            "../../../assets/instruments/electric_guitar.svg"
        ))
        .unwrap_or_default(),
        StemKind::Piano => {
            Image::load_from_svg_data(include_bytes!("../../../assets/instruments/keys.svg"))
                .unwrap_or_default()
        }
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

fn populate_track_detail(tracks_window: &TracksWindow, entry: &TrackEntry) {
    tracks_window.set_track_detail_title(entry.meta.title.as_str().into());
    tracks_window.set_track_detail_artist(
        entry
            .meta
            .artist
            .as_deref()
            .unwrap_or("")
            .to_string()
            .into(),
    );
    tracks_window.set_track_detail_bpm_text(
        entry
            .meta
            .bpm
            .map(|b| format!("{b:.0}"))
            .unwrap_or_else(|| "—".to_string())
            .into(),
    );
    tracks_window
        .set_track_detail_key_text(entry.meta.key.as_deref().unwrap_or("—").to_string().into());
    tracks_window.set_track_detail_duration_text(format_duration(entry.meta.duration_secs));
    tracks_window.set_track_detail_position_text(format_duration(0.0));

    let stems: Vec<StemRowData> = entry
        .meta
        .stems
        .iter()
        .enumerate()
        .map(|(idx, stem)| {
            let peaks_path = entry.stem_peaks_path(stem.kind);
            let peaks = Image::load_from_path(&peaks_path).unwrap_or_default();
            StemRowData {
                index: idx as i32,
                label: stem_kind_label(stem.kind).into(),
                icon: stem_kind_icon(stem.kind),
                peaks,
                muted: false,
                soloed: false,
                gain: 1.0,
                pan: 0.0,
            }
        })
        .collect();
    let model = Rc::new(VecModel::from(stems));
    tracks_window.set_track_detail_stems(ModelRc::from(model));
    tracks_window.set_track_detail_playing(false);
    tracks_window.set_show_track_detail(true);
}

fn dispatch_separation(
    tracks_window: &TracksWindow,
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
                        spawn_separation_worker(tracks_window, source_path);
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
        spawn_separation_worker(tracks_window, source_path);
    }
}

fn spawn_separation_worker(tracks_window: &TracksWindow, source_path: PathBuf) {
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

    let tw_weak: Weak<TracksWindow> = tracks_window.as_weak();
    thread::spawn(move || match feature_stems::separate_track(&request) {
        Ok(_) => {
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(tw) = tw_weak.upgrade() {
                    refresh_only_list(&tw);
                }
            });
        }
        Err(err) => {
            eprintln!("tracks: separation failed: {err}");
        }
    });
}

fn refresh_only_list(tracks_window: &TracksWindow) {
    let dir = default_tracks_dir();
    let rows: Vec<TrackRowData> = feature_tracks::scan_catalog(&dir)
        .unwrap_or_default()
        .iter()
        .map(entry_to_row)
        .collect();
    let model = Rc::new(VecModel::from(rows));
    tracks_window.set_tracks(ModelRc::from(model));
}

/// Wire the Tracks secondary window. Mirrors the Tuner/Spectrum
/// pattern: own native window, opened by a header callback on the main
/// window, closed by the window's own back/close action.
pub(crate) fn wire_tracks_window(
    main_window: &AppWindow,
    tracks_window: &TracksWindow,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
) {
    let state = Rc::new(RefCell::new(TracksState::new()));
    rescan_catalog(&state, tracks_window);
    tracks_window.set_tracks_search_text(SharedString::new());
    tracks_window.set_show_track_detail(false);

    // 30 Hz playhead poll → updates the position label + 0..1
    // progress so the waveform line moves while a stem is playing.
    let playhead_timer = Timer::default();
    {
        let state = state.clone();
        let tw_weak = tracks_window.as_weak();
        playhead_timer.start(
            TimerMode::Repeated,
            std::time::Duration::from_millis(33),
            move || {
                let Some(tw) = tw_weak.upgrade() else { return };
                let snapshot = state.borrow();
                let Some(player) = snapshot.player.as_ref() else {
                    return;
                };
                let total = snapshot.total_frames;
                let head = player.playhead();
                let secs = if player.sample_rate() > 0 {
                    head as f64 / player.sample_rate() as f64
                } else {
                    0.0
                };
                tw.set_track_detail_position_text(format_duration(secs));
                let progress = if total > 0 {
                    (head as f32 / total as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                tw.set_track_detail_progress(progress);
            },
        );
    }
    // Keep the timer alive for the whole window lifetime by leaking
    // it into the closure that holds it. (Timer is not Drop-cancelled
    // when the window closes; this is intentional — re-opening the
    // window reuses the same Slint event loop.)
    Box::leak(Box::new(playhead_timer));

    // Open: main window → show TracksWindow as a child window.
    {
        let state = state.clone();
        let main_weak = main_window.as_weak();
        let tw_weak = tracks_window.as_weak();
        main_window.on_open_tracks_clicked(move || {
            let Some(main_w) = main_weak.upgrade() else {
                return;
            };
            let Some(tw) = tw_weak.upgrade() else {
                return;
            };
            tw.set_show_track_detail(false);
            rescan_catalog(&state, &tw);
            show_child_window(main_w.window(), tw.window());
        });
    }

    // Back / close: hide the window.
    {
        let tw_weak = tracks_window.as_weak();
        tracks_window.on_close_tracks(move || {
            if let Some(tw) = tw_weak.upgrade() {
                tw.window().hide().ok();
            }
        });
    }

    // Import → file picker → separation worker.
    {
        let tw_weak = tracks_window.as_weak();
        let project_session = project_session.clone();
        tracks_window.on_tracks_import_clicked(move || {
            let file = rfd::FileDialog::new()
                .add_filter("Audio", &["wav", "mp3", "flac", "ogg", "m4a"])
                .pick_file();
            let Some(path) = file else { return };
            if !path.exists() || !path.is_file() {
                return;
            }
            if let Some(tw) = tw_weak.upgrade() {
                dispatch_separation(&tw, &project_session, path);
            }
        });
    }

    // Track click → open detail in same window.
    {
        let state = state.clone();
        let tw_weak = tracks_window.as_weak();
        tracks_window.on_tracks_track_clicked(move |id| {
            let Some(tw) = tw_weak.upgrade() else { return };
            let entry = state
                .borrow()
                .catalog
                .iter()
                .find(|e| e.meta.id.as_str() == id.as_str())
                .cloned();
            let Some(entry) = entry else { return };

            let stems = load_stems_for_entry(&entry);
            let total_frames = stems.iter().map(|s| s.len() / 2).min().unwrap_or(0);
            let player = MultiStemPlayer::new(stems, entry.meta.source_sample_rate);
            populate_track_detail(&tw, &entry);
            let mut s = state.borrow_mut();
            s.player = Some(Arc::new(player));
            s.total_frames = total_frames;
            tw.set_track_detail_progress(0.0);
        });
    }

    // Back inside detail → list.
    {
        let state = state.clone();
        let tw_weak = tracks_window.as_weak();
        tracks_window.on_track_detail_back_clicked(move || {
            if let Some(tw) = tw_weak.upgrade() {
                let mut s = state.borrow_mut();
                s.stream = None;
                s.player = None;
                s.total_frames = 0;
                tw.set_track_detail_progress(0.0);
                tw.set_show_track_detail(false);
            }
        });
    }

    // Per-stem controls.
    {
        let state = state.clone();
        tracks_window.on_track_detail_stem_mute_toggled(move |idx, muted| {
            if let Some(player) = state.borrow().player.as_ref() {
                player.set_mute(idx as usize, muted);
            }
        });
    }
    {
        let state = state.clone();
        tracks_window.on_track_detail_stem_solo_toggled(move |idx, soloed| {
            if let Some(player) = state.borrow().player.as_ref() {
                player.set_solo(idx as usize, soloed);
            }
        });
    }
    {
        let state = state.clone();
        tracks_window.on_track_detail_stem_gain_changed(move |idx, gain| {
            if let Some(player) = state.borrow().player.as_ref() {
                player.set_gain(idx as usize, gain);
            }
        });
    }
    {
        let state = state.clone();
        tracks_window.on_track_detail_stem_pan_changed(move |idx, pan| {
            if let Some(player) = state.borrow().player.as_ref() {
                player.set_pan(idx as usize, pan);
            }
        });
    }

    // Reflect mute/solo back into the row model.
    {
        let tw_weak = tracks_window.as_weak();
        tracks_window.on_track_detail_stem_mute_toggled(move |idx, muted| {
            if let Some(tw) = tw_weak.upgrade() {
                let model = tw.get_track_detail_stems();
                if let Some(mut row) = model.row_data(idx as usize) {
                    row.muted = muted;
                    model.set_row_data(idx as usize, row);
                }
            }
        });
    }
    {
        let tw_weak = tracks_window.as_weak();
        tracks_window.on_track_detail_stem_solo_toggled(move |idx, soloed| {
            if let Some(tw) = tw_weak.upgrade() {
                let model = tw.get_track_detail_stems();
                if let Some(mut row) = model.row_data(idx as usize) {
                    row.soloed = soloed;
                    model.set_row_data(idx as usize, row);
                }
            }
        });
    }

    // Play / pause → cpal stream.
    {
        let state = state.clone();
        let tw_weak = tracks_window.as_weak();
        tracks_window.on_track_detail_play_toggle(move || {
            let Some(tw) = tw_weak.upgrade() else { return };
            let target_playing = !tw.get_track_detail_playing();
            let mut s = state.borrow_mut();
            if target_playing {
                if let Some(player) = s.player.clone() {
                    match crate::tracks_player_stream::TrackPlaybackStream::start(player) {
                        Ok(stream) => {
                            s.stream = Some(stream);
                            tw.set_track_detail_playing(true);
                        }
                        Err(err) => {
                            eprintln!("tracks: cannot start playback: {err}");
                        }
                    }
                }
            } else {
                s.stream = None;
                tw.set_track_detail_playing(false);
            }
        });
    }

    // Seek currently a no-op until the cpal stream exposes a playhead
    // hook; the UI binding stays wired for forward compatibility.
    tracks_window.on_track_detail_seek_relative(|_delta| {});
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
