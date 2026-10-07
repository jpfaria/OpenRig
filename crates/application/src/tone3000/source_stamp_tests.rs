use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;
use crate::tone3000::manifest_build::CaptureFile;

fn tone() -> Tone {
    serde_json::from_value(serde_json::json!({
        "id": 42, "title": "Cab", "updated_at": "2026-05-01T10:00:00Z"
    }))
    .unwrap()
}

fn capture(name: &str, file: &str) -> CaptureFile {
    CaptureFile {
        name: name.into(),
        file: file.into(),
        output_gain_db: None,
    }
}

#[test]
fn the_stamp_keeps_the_tone3000_name_of_every_capture_file() {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        capture("Cab SM57 Cap", "captures/000.wav"),
        capture("Cab SM57 Cone", "captures/001.wav"),
    ];

    write_stamp(dir.path(), &tone(), &files).unwrap();

    assert_eq!(
        read_capture_names(dir.path()),
        BTreeMap::from([
            (
                PathBuf::from("captures/000.wav"),
                "Cab SM57 Cap".to_string()
            ),
            (
                PathBuf::from("captures/001.wav"),
                "Cab SM57 Cone".to_string()
            ),
        ])
    );
    assert_eq!(
        read_updated_at(dir.path()).as_deref(),
        Some("2026-05-01T10:00:00Z")
    );
}

#[test]
fn a_stamp_written_before_names_were_kept_still_reads() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(STAMP_FILE),
        "tone_id: 42\nupdated_at: '2026-05-01T10:00:00Z'\n",
    )
    .unwrap();

    assert!(read_capture_names(dir.path()).is_empty());
    assert_eq!(
        read_updated_at(dir.path()).as_deref(),
        Some("2026-05-01T10:00:00Z")
    );
}
