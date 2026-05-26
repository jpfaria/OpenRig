//! RED-first tests for the tracks catalog layer.

use std::fs;

use feature_tracks::{
    scan_catalog, StemInfo, StemKind, TrackEntry, TrackId, TrackMeta, TracksError,
};

fn fixture_meta(id: &str, title: &str) -> TrackMeta {
    TrackMeta {
        id: TrackId::new(id),
        title: title.to_string(),
        artist: Some("Test Artist".to_string()),
        album: Some("Test Album".to_string()),
        year: Some(2026),
        genre: Some("rock".to_string()),
        bpm: Some(120.0),
        key: Some("Am".to_string()),
        duration_secs: 180.0,
        source_sample_rate: 44_100,
        stems: vec![
            StemInfo {
                kind: StemKind::Drums,
                filename: "drums.wav".to_string(),
            },
            StemInfo {
                kind: StemKind::Bass,
                filename: "bass.wav".to_string(),
            },
            StemInfo {
                kind: StemKind::Vocals,
                filename: "vocals.wav".to_string(),
            },
            StemInfo {
                kind: StemKind::Other,
                filename: "other.wav".to_string(),
            },
        ],
        model: "htdemucs".to_string(),
        generated_at: "2026-05-26T12:00:00Z".to_string(),
    }
}

#[test]
fn track_meta_save_load_roundtrip_preserves_every_field() {
    let dir = tempfile::tempdir().expect("tempdir");
    let track_dir = dir.path().join("01HXEMP0TEST");
    fs::create_dir_all(&track_dir).expect("create track dir");

    let meta = fixture_meta("01HXEMP0TEST", "Clocks (Live)");
    let entry = TrackEntry {
        meta: meta.clone(),
        dir: track_dir.clone(),
    };
    entry.save().expect("save meta");

    assert!(track_dir.join("meta.yaml").exists());

    let loaded = TrackEntry::load(&track_dir).expect("load meta");
    assert_eq!(loaded.meta, meta);
    assert_eq!(loaded.dir, track_dir);
}

#[test]
fn track_entry_resolves_stem_paths_relative_to_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let track_dir = dir.path().join("01HXEMP1TEST");
    let entry = TrackEntry {
        meta: fixture_meta("01HXEMP1TEST", "Test"),
        dir: track_dir.clone(),
    };

    assert_eq!(
        entry.stem_path(StemKind::Drums),
        track_dir.join("drums.wav")
    );
    assert_eq!(entry.stem_path(StemKind::Bass), track_dir.join("bass.wav"));
    assert_eq!(
        entry.stem_path(StemKind::Vocals),
        track_dir.join("vocals.wav")
    );
    assert_eq!(
        entry.stem_path(StemKind::Other),
        track_dir.join("other.wav")
    );
    assert_eq!(entry.peaks_path(), track_dir.join("peaks.bin"));
}

#[test]
fn load_missing_meta_yaml_returns_meta_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let track_dir = dir.path().join("01HXEMPMISSING");
    fs::create_dir_all(&track_dir).expect("create track dir");

    let err = TrackEntry::load(&track_dir).expect_err("must fail when meta missing");
    assert!(
        matches!(err, TracksError::Meta { .. }),
        "expected Meta error, got {err:?}"
    );
}

#[test]
fn load_malformed_meta_yaml_returns_meta_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let track_dir = dir.path().join("01HXEMPBAD");
    fs::create_dir_all(&track_dir).expect("create track dir");
    fs::write(track_dir.join("meta.yaml"), "not: valid: yaml: at: all").expect("write yaml");

    let err = TrackEntry::load(&track_dir).expect_err("must fail on malformed yaml");
    assert!(
        matches!(err, TracksError::Meta { .. }),
        "expected Meta error, got {err:?}"
    );
}

#[test]
fn scan_catalog_returns_all_valid_entries_and_skips_unrelated_dirs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let catalog_dir = dir.path().join("tracks");
    fs::create_dir_all(&catalog_dir).expect("create catalog");

    for (id, title) in [
        ("01HXEMPA0001", "Track A"),
        ("01HXEMPA0002", "Track B"),
        ("01HXEMPA0003", "Track C"),
    ] {
        let track_dir = catalog_dir.join(id);
        fs::create_dir_all(&track_dir).expect("create track");
        TrackEntry {
            meta: fixture_meta(id, title),
            dir: track_dir,
        }
        .save()
        .expect("save");
    }

    // Sibling dir without a meta.yaml — must be skipped silently.
    fs::create_dir_all(catalog_dir.join("not_a_track")).expect("create non-track");
    // Plain file — must be skipped silently.
    fs::write(catalog_dir.join("random.txt"), "hello").expect("write file");

    let entries = scan_catalog(&catalog_dir).expect("scan");
    assert_eq!(entries.len(), 3, "must find exactly the 3 valid tracks");
    let mut ids: Vec<String> = entries
        .into_iter()
        .map(|e| e.meta.id.as_str().to_string())
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "01HXEMPA0001".to_string(),
            "01HXEMPA0002".to_string(),
            "01HXEMPA0003".to_string(),
        ]
    );
}

#[test]
fn scan_catalog_on_missing_directory_returns_empty_vec() {
    let dir = tempfile::tempdir().expect("tempdir");
    let catalog_dir = dir.path().join("tracks_never_created");

    let entries = scan_catalog(&catalog_dir).expect("scan missing");
    assert!(
        entries.is_empty(),
        "missing catalog must be treated as empty"
    );
}
