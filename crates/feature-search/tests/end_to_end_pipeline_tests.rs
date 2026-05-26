//! End-to-end #553 integration test.
//!
//! Drives every #553 crate through one canonical flow: synthesize a
//! source WAV, run `feature_stems::separate_track` to populate the
//! catalog, scan the catalog with `feature_tracks::scan_catalog`, feed
//! the result into `feature_search::TracksIndex::rebuild_tracks`, and
//! finally validate that a search by title returns the new track and
//! a `delete_track` removes it.

use std::path::Path;

use feature_search::{SearchQuery, TracksIndex};
use feature_stems::{separate_track, SeparateRequest};
use feature_tracks::scan_catalog;
use hound::{SampleFormat, WavSpec, WavWriter};

fn write_sine_wav(path: &Path, sample_rate: u32, channels: u16, secs: u32, freq_hz: f32) {
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec).expect("create wav writer");
    let total = (sample_rate * secs) as usize;
    for frame in 0..total {
        let t = frame as f32 / sample_rate as f32;
        let s = (t * freq_hz * std::f32::consts::TAU).sin() * 0.5;
        for _ in 0..channels {
            writer.write_sample(s).expect("write sample");
        }
    }
    writer.finalize().expect("finalize");
}

#[test]
fn full_pipeline_decode_separate_scan_index_search_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("integration_song.wav");
    write_sine_wav(&source, 48_000, 2, 1, 440.0);

    let catalog_dir = dir.path().join("tracks");
    let track_id = "01HXEMP_INTEGRATION";

    let request = SeparateRequest {
        source_path: source.clone(),
        catalog_dir: catalog_dir.clone(),
        track_id: track_id.to_string(),
        title: "Integration Song".to_string(),
        model: "stub".to_string(),
        generated_at: "2026-05-26T12:00:00Z".to_string(),
    };

    let entry = separate_track(&request).expect("separate_track");
    assert_eq!(entry.meta.title, "Integration Song");
    assert!(entry.dir.join("meta.yaml").exists());
    assert!(entry.dir.join("drums.wav").exists());
    assert!(entry.dir.join("bass.wav").exists());
    assert!(entry.dir.join("vocals.wav").exists());
    assert!(entry.dir.join("other.wav").exists());

    let scanned = scan_catalog(&catalog_dir).expect("scan_catalog");
    assert_eq!(scanned.len(), 1, "scan must find the freshly written track");

    let index_dir = dir.path().join("index");
    let index = TracksIndex::open(&index_dir).expect("open index");
    index.rebuild_tracks(scanned.iter()).expect("index rebuild");

    let hits = index
        .search(&SearchQuery::text("Integration"))
        .expect("search");
    assert!(
        hits.iter().any(|h| h.id == track_id),
        "the integration track must surface in the search index"
    );

    index.delete_track(track_id).expect("delete");
    let after_delete = index
        .search(&SearchQuery::text("Integration"))
        .expect("search after delete");
    assert!(
        after_delete.iter().all(|h| h.id != track_id),
        "deleted track must not surface in subsequent searches"
    );
}
