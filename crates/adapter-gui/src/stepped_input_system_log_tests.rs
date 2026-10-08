//! #1081: every stepped-input mark keeps the system's audio log of the
//! minutes before the trip, so the next episode shows what the system did,
//! whatever it was.

use std::io;

use super::{audio_log_args, write_system_log, LOG_FILE};

#[test]
fn the_mark_keeps_the_system_audio_log() {
    let dir = tempfile::tempdir().unwrap();
    write_system_log(dir.path(), &|| {
        Ok("io started\nevent link timed out\n".into())
    })
    .unwrap();
    let kept = std::fs::read_to_string(dir.path().join(LOG_FILE)).unwrap();
    assert_eq!(kept, "io started\nevent link timed out\n");
}

#[test]
fn a_log_that_cannot_be_read_is_reported_and_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let result = write_system_log(dir.path(), &|| Err(io::Error::other("no log")));
    assert!(result.is_err());
    assert!(!dir.path().join(LOG_FILE).exists());
}

#[test]
fn the_log_covers_the_five_minutes_before_the_trip_from_the_audio_daemon_and_its_clients() {
    let args = audio_log_args();
    let last = args.iter().position(|a| a == "--last").unwrap();
    assert_eq!(args[last + 1], "300s");
    let predicate = args.last().unwrap();
    assert!(predicate.contains("coreaudiod"));
    assert!(predicate.contains("com.apple.coreaudio"));
}
