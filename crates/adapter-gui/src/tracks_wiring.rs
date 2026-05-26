//! #553 — Tracks screen GUI wiring.
//!
//! Hooks the Launcher's "Tracks" button, the Tracks page's Back
//! button, and the Import button. Catalog discovery + the off-RT
//! separation worker are kicked off here so the user can actually
//! load audio and see stems land in the catalog without leaving the
//! app. The Track detail view (multi-stem player) lands in a
//! follow-up.

use std::path::PathBuf;
use std::rc::Rc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use feature_stems::SeparateRequest;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use uuid::Uuid;

use crate::{AppWindow, TrackRowData};

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

fn entry_to_row(entry: &feature_tracks::TrackEntry) -> TrackRowData {
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

/// Scan the tracks catalog and replace the Slint model with the
/// resulting rows. Failures collapse to an empty model so the
/// empty-state message renders cleanly.
pub(crate) fn refresh_tracks_model(window: &AppWindow) {
    let dir = default_tracks_dir();
    let rows: Vec<TrackRowData> = feature_tracks::scan_catalog(&dir)
        .unwrap_or_default()
        .iter()
        .map(entry_to_row)
        .collect();
    let model = Rc::new(VecModel::from(rows));
    window.set_tracks(ModelRc::from(model));
}

/// ISO-8601 UTC timestamp without external crates. Drops sub-second
/// precision so the `meta.yaml` reads cleanly.
fn current_utc_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();

    // Days since epoch + civil-from-days (Howard Hinnant) for a
    // dependency-free YYYY-MM-DD conversion.
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Convert "days since 1970-01-01" to a `(year, month, day)` triple
/// using Howard Hinnant's `civil_from_days` algorithm. Works for any
/// realistic timestamp this app will ever see.
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
                    refresh_tracks_model(&window);
                }
            });
        }
        Err(err) => {
            eprintln!("tracks: separation failed: {err}");
        }
    });
}

/// Connect the Launcher → Tracks → Launcher navigation and the Import
/// button. Runs an initial catalog scan at startup so the list is
/// populated by the time the user opens the screen.
pub(crate) fn wire_tracks_nav(window: &AppWindow) {
    refresh_tracks_model(window);

    let window_weak_open = window.as_weak();
    window.on_open_tracks_clicked(move || {
        if let Some(window) = window_weak_open.upgrade() {
            refresh_tracks_model(&window);
            window.set_show_project_launcher(false);
            window.set_show_tracks(true);
        }
    });

    let window_weak_back = window.as_weak();
    window.on_tracks_back_clicked(move || {
        if let Some(window) = window_weak_back.upgrade() {
            window.set_show_tracks(false);
            window.set_show_project_launcher(true);
        }
    });

    let window_weak_import = window.as_weak();
    window.on_tracks_import_clicked(move || {
        let file = rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "mp3", "flac", "ogg", "m4a"])
            .pick_file();
        let Some(path) = file else { return };
        if !path.exists() || !path.is_file() {
            return;
        }
        if let Some(window) = window_weak_import.upgrade() {
            spawn_separation_worker(&window, path);
        }
    });

    window.set_tracks_search_text(SharedString::new());
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
        // 2026-05-26 is 20_599 days after 1970-01-01.
        assert_eq!(civil_from_days(20_599), (2026, 5, 26));
        // Leap-day round-trip — 2020-02-29 = day 18_321.
        assert_eq!(civil_from_days(18_321), (2020, 2, 29));
    }

    #[test]
    fn current_utc_iso8601_emits_well_formed_timestamp() {
        let ts = current_utc_iso8601();
        // YYYY-MM-DDTHH:MM:SSZ → 20 chars.
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[10..11], "T");
        assert!(ts.ends_with('Z'));
    }
}
