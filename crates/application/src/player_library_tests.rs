use super::*;
use crate::player_track_category::TrackCategory;

fn touch(dir: &Path, name: &str) {
    std::fs::write(dir.join(name), b"x").expect("write test file");
}

fn names(tracks: &[BackingTrack]) -> Vec<&str> {
    tracks.iter().map(|t| t.name.as_str()).collect()
}

#[test]
fn supported_extensions_are_recognised_in_any_case() {
    for name in ["a.wav", "a.FLAC", "a.mp3", "a.ogg", "a.M4A", "a.aac"] {
        assert!(is_backing_track_file(Path::new(name)), "{name}");
    }
    for name in ["a.txt", "a", "a.wav.part", ".m4a"] {
        assert!(!is_backing_track_file(Path::new(name)), "{name}");
    }
}

#[test]
fn bundled_tracks_come_first_then_the_users() {
    let bundled = tempfile::tempdir().unwrap();
    let user = tempfile::tempdir().unwrap();
    touch(bundled.path(), "Slow Blues in A.m4a");
    touch(bundled.path(), "funk in E.m4a");
    touch(user.path(), "My Jam.wav");
    let tracks = list_backing_tracks(&PlayerLibraryDirs {
        bundled: Some(bundled.path().into()),
        user: Some(user.path().into()),
    });
    assert_eq!(names(&tracks), ["funk in E", "Slow Blues in A", "My Jam"]);
    assert!(tracks[0].bundled && tracks[1].bundled && !tracks[2].bundled);
    assert_eq!(tracks[2].path, user.path().join("My Jam.wav"));
}

#[test]
fn hidden_and_unsupported_files_are_skipped() {
    let user = tempfile::tempdir().unwrap();
    touch(user.path(), ".hidden.wav");
    touch(user.path(), "notes.txt");
    touch(user.path(), "take.flac");
    std::fs::create_dir(user.path().join("folder.wav")).unwrap();
    let tracks = list_backing_tracks(&PlayerLibraryDirs {
        bundled: None,
        user: Some(user.path().into()),
    });
    assert_eq!(names(&tracks), ["take"]);
}

#[test]
fn missing_folders_list_nothing() {
    let tracks = list_backing_tracks(&PlayerLibraryDirs {
        bundled: Some("/definitely/not/here".into()),
        user: None,
    });
    assert!(tracks.is_empty());
}

fn touch_in(dir: &Path, folder: &str, name: &str) {
    let sub = dir.join(folder);
    std::fs::create_dir_all(&sub).expect("create category folder");
    touch(&sub, name);
}

fn category_of(tracks: &[BackingTrack], name: &str) -> TrackCategory {
    tracks
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("{name} not listed"))
        .category
}

#[test]
fn a_track_takes_the_category_of_its_folder() {
    let bundled = tempfile::tempdir().unwrap();
    let user = tempfile::tempdir().unwrap();
    touch_in(bundled.path(), "solo", "Slow Blues - A.m4a");
    touch_in(bundled.path(), "rhythm", "Drums and Bass - E.m4a");
    touch_in(user.path(), "Bass", "No Bass Funk.wav");
    touch_in(user.path(), "acoustic", "Campfire - G.mp3");
    touch_in(user.path(), "misc", "Odd One.wav");
    touch(user.path(), "Loose.wav");
    let tracks = list_backing_tracks(&PlayerLibraryDirs {
        bundled: Some(bundled.path().into()),
        user: Some(user.path().into()),
    });
    assert_eq!(category_of(&tracks, "Slow Blues - A"), TrackCategory::Solo);
    assert_eq!(
        category_of(&tracks, "Drums and Bass - E"),
        TrackCategory::Rhythm
    );
    assert_eq!(category_of(&tracks, "No Bass Funk"), TrackCategory::Bass);
    assert_eq!(
        category_of(&tracks, "Campfire - G"),
        TrackCategory::Acoustic
    );
    assert_eq!(category_of(&tracks, "Odd One"), TrackCategory::Other);
    assert_eq!(category_of(&tracks, "Loose"), TrackCategory::Other);
    assert_eq!(
        tracks
            .iter()
            .find(|t| t.name == "Campfire - G")
            .unwrap()
            .path,
        user.path().join("acoustic").join("Campfire - G.mp3")
    );
}
