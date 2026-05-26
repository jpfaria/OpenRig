//! RED-first tests for metadata extraction via `lofty`.

use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};

fn write_silent_wav(path: &Path) {
    let spec = WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec).expect("create wav writer");
    for _ in 0..1024 {
        writer.write_sample(0.0_f32).expect("L");
        writer.write_sample(0.0_f32).expect("R");
    }
    writer.finalize().expect("finalize");
}

#[test]
fn extract_tags_returns_default_when_file_has_no_metadata() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("no_tags.wav");
    write_silent_wav(&path);

    let tags = feature_stems::extract_tags(&path).expect("extract");
    assert_eq!(tags.title, None);
    assert_eq!(tags.artist, None);
    assert_eq!(tags.album, None);
    assert_eq!(tags.year, None);
    assert_eq!(tags.genre, None);
}

#[test]
fn extract_tags_for_missing_path_returns_default_tags() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does_not_exist.mp3");

    let tags = feature_stems::extract_tags(&missing).expect("extract on missing");
    assert_eq!(tags.title, None);
    assert_eq!(tags.artist, None);
}
