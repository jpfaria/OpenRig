//! #323 — the loop audio sidecar: where a recorded loop is written and how it
//! comes back.

use super::*;

/// A throwaway directory that cleans itself up — never a path from this
/// machine.
fn tmp_dir(_name: &str) -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir")
}

#[test]
fn loops_live_next_to_the_project_in_a_named_folder() {
    let project = std::path::Path::new("/rigs/live.openrig");
    let path = loop_file_path(project, &domain::ids::ChainId("chain:7".into()), 3);

    assert_eq!(
        path.parent().unwrap(),
        std::path::Path::new("/rigs/live.loops")
    );
    assert_eq!(
        path.file_name().unwrap().to_string_lossy(),
        "chain-7-looper-3.wav",
        "the file name is derived from chain + looper, sanitised for disk"
    );
}

#[test]
fn a_saved_loop_round_trips_at_its_own_rate() {
    let dir = tmp_dir("looper-roundtrip");
    let project = dir.path().join("song.openrig");
    let chain = domain::ids::ChainId("chain:1".into());
    let pcm = vec![0.0, 0.0, 0.5, -0.5, 1.0, -1.0];

    let file = write_loop_wav(&project, &chain, 1, &pcm, 44_100).expect("writes");
    assert!(dir.path().join("song.loops").join(&file).exists());

    let (back, rate) = read_loop_wav(&project, &file).expect("reads");
    assert_eq!(rate, 44_100);
    assert_eq!(back.len(), pcm.len());
    for (a, b) in back.iter().zip(&pcm) {
        assert!((a - b).abs() < 1e-4, "{a} != {b}");
    }
}

#[test]
fn reading_a_missing_loop_is_an_error_not_a_panic() {
    let dir = tmp_dir("looper-missing");
    let project = dir.path().join("song.openrig");
    assert!(read_loop_wav(&project, "nope.wav").is_err());
}

#[test]
fn a_loop_recorded_at_another_rate_is_resampled_to_the_engine_rate() {
    // 4 frames at 24 kHz become 8 frames at 48 kHz — the loop keeps its
    // duration instead of playing at double speed (#669's lesson).
    let pcm = vec![0.0, 0.0, 1.0, 1.0, 0.0, 0.0, -1.0, -1.0];
    let resampled = resample_loop(&pcm, 24_000, 48_000);
    assert_eq!(resampled.len(), 16);
}

#[test]
fn a_loop_already_at_the_engine_rate_is_returned_untouched() {
    let pcm = vec![0.25, -0.25, 0.5, -0.5];
    assert_eq!(resample_loop(&pcm, 48_000, 48_000), pcm);
}
