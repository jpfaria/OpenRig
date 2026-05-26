//! RED-first tests for the tracks search index.

use std::path::PathBuf;

use feature_search::{SearchQuery, TracksIndex};
use feature_tracks::{StemInfo, StemKind, TrackEntry, TrackId, TrackMeta};

fn fixture_entry(id: &str, title: &str, artist: &str, genre: &str) -> TrackEntry {
    TrackEntry {
        meta: TrackMeta {
            id: TrackId::new(id),
            title: title.to_string(),
            artist: Some(artist.to_string()),
            album: Some("Some Album".to_string()),
            year: Some(2026),
            genre: Some(genre.to_string()),
            bpm: Some(120.0),
            key: Some("Am".to_string()),
            duration_secs: 240.0,
            source_sample_rate: 44_100,
            stems: vec![StemInfo {
                kind: StemKind::Drums,
                filename: "drums.wav".to_string(),
            }],
            model: "stub".to_string(),
            generated_at: "2026-05-26T12:00:00Z".to_string(),
        },
        dir: PathBuf::from("/tmp/tracks").join(id),
    }
}

#[test]
fn empty_index_returns_no_hits() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = TracksIndex::open(dir.path()).expect("open");
    let hits = index
        .search(&SearchQuery::text("anything"))
        .expect("search");
    assert!(hits.is_empty());
}

#[test]
fn rebuild_then_text_search_finds_matching_titles() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = TracksIndex::open(dir.path()).expect("open");
    let entries = [
        fixture_entry("01HXTREE001", "Clocks Live", "Coldplay", "rock"),
        fixture_entry("01HXTREE002", "Yellow", "Coldplay", "rock"),
        fixture_entry("01HXTREE003", "Wonderwall", "Oasis", "rock"),
    ];
    index.rebuild_tracks(entries.iter()).expect("rebuild");

    let hits = index.search(&SearchQuery::text("Clocks")).expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "01HXTREE001");
    assert_eq!(hits[0].title, "Clocks Live");
}

#[test]
fn artist_facet_filters_to_matching_artist_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = TracksIndex::open(dir.path()).expect("open");
    let entries = [
        fixture_entry("01HXTREE010", "Clocks", "Coldplay", "rock"),
        fixture_entry("01HXTREE011", "Yellow", "Coldplay", "rock"),
        fixture_entry("01HXTREE012", "Wonderwall", "Oasis", "rock"),
    ];
    index.rebuild_tracks(entries.iter()).expect("rebuild");

    let mut q = SearchQuery::text("");
    q.artist = Some("Coldplay".to_string());
    let hits = index.search(&q).expect("search");
    let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"01HXTREE010"));
    assert!(ids.contains(&"01HXTREE011"));
}

#[test]
fn delete_track_removes_from_subsequent_search() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = TracksIndex::open(dir.path()).expect("open");
    let entries = [
        fixture_entry("01HXTREE020", "First", "Artist A", "rock"),
        fixture_entry("01HXTREE021", "Second", "Artist A", "rock"),
    ];
    index.rebuild_tracks(entries.iter()).expect("rebuild");

    index.delete_track("01HXTREE020").expect("delete");

    let hits = index
        .search(&SearchQuery::text("First"))
        .expect("search after delete");
    assert!(hits.is_empty(), "deleted track must not appear in hits");
}

#[test]
fn second_rebuild_includes_new_payload_in_search_results() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = TracksIndex::open(dir.path()).expect("open");
    let initial = [fixture_entry(
        "01HXTREE030",
        "Old Title",
        "Old Artist",
        "rock",
    )];
    index
        .rebuild_tracks(initial.iter())
        .expect("rebuild initial");

    let replaced = [fixture_entry(
        "01HXTREE030",
        "New Title",
        "New Artist",
        "rock",
    )];
    index
        .rebuild_tracks(replaced.iter())
        .expect("rebuild replaced");

    let hits = index
        .search(&SearchQuery::text("New Title"))
        .expect("search");
    assert!(
        hits.iter()
            .any(|h| h.id == "01HXTREE030" && h.title == "New Title"),
        "rebuild must add the new payload, got hits: {hits:?}"
    );
}
