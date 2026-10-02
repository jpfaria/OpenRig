use super::*;

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
