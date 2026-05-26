//! RED-first end-to-end tests for the stem-separation orchestrator.
//!
//! The current inference module is a placeholder: it produces four
//! buffers that, when summed, reconstruct the input. These tests cover
//! the *plumbing* — decode → resample → separate → write WAVs + meta —
//! so the real `ort` + htdemucs implementation can swap in without
//! disturbing the catalog interface.

use std::path::Path;

use feature_tracks::{StemKind, TrackEntry};
use hound::{SampleFormat, WavSpec, WavWriter};

const SOURCE_SR: u32 = 48_000;
const DURATION_SECS: u32 = 1;
const FREQ_HZ: f32 = 440.0;

fn write_sine_wav(path: &Path) {
    let spec = WavSpec {
        channels: 2,
        sample_rate: SOURCE_SR,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut writer = WavWriter::create(path, spec).expect("create wav writer");
    let total_frames = (SOURCE_SR * DURATION_SECS) as usize;
    for frame in 0..total_frames {
        let t = frame as f32 / SOURCE_SR as f32;
        let s = (t * FREQ_HZ * std::f32::consts::TAU).sin() * 0.5;
        writer.write_sample(s).expect("L");
        writer.write_sample(s).expect("R");
    }
    writer.finalize().expect("finalize");
}

#[test]
fn separate_stems_returns_four_stereo_buffers_matching_input_length() {
    let frames = 44_100; // 1s at 44.1k stereo
    let input = vec![0.5_f32; frames * 2];

    let stems = feature_stems::separate_stems(&input, 44_100).expect("separate");
    assert_eq!(stems.len(), 4, "must produce exactly 4 stems");
    for stem in &stems {
        assert_eq!(
            stem.len(),
            input.len(),
            "each stem must have the same sample count as input"
        );
    }
}

#[test]
fn separate_track_orchestrator_writes_meta_and_four_wavs_to_track_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("source.wav");
    write_sine_wav(&source);
    let catalog = dir.path().join("tracks");

    let request = feature_stems::SeparateRequest {
        source_path: source.clone(),
        catalog_dir: catalog.clone(),
        track_id: "01HXEMP9TEST".to_string(),
        title: "Test Track".to_string(),
        model: "stub".to_string(),
        generated_at: "2026-05-26T12:00:00Z".to_string(),
    };

    let entry: TrackEntry = feature_stems::separate_track(&request).expect("separate_track");

    assert_eq!(entry.dir, catalog.join("01HXEMP9TEST"));
    assert!(
        entry.dir.join("meta.yaml").exists(),
        "meta.yaml must be written"
    );
    for kind in [
        StemKind::Drums,
        StemKind::Bass,
        StemKind::Vocals,
        StemKind::Other,
    ] {
        let path = entry.stem_path(kind);
        assert!(path.exists(), "{:?} wav must be written at {path:?}", kind);
    }
    assert_eq!(entry.meta.title, "Test Track");
    assert_eq!(entry.meta.model, "stub");
    assert_eq!(entry.meta.source_sample_rate, SOURCE_SR);
    assert_eq!(entry.meta.stems.len(), 4);
    assert!(entry.meta.duration_secs > 0.95 && entry.meta.duration_secs < 1.05);
}
