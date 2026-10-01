use std::path::Path;
use std::time::{Duration, SystemTime};

use infra_cpal::{
    CycleRecord, DeviceProbe, InputEvidenceSnapshot, InputStreamIdentity, SteppedInputEvidence,
    StreamEvidence,
};

use super::{prune_marks, write_after_probe, write_mark};

/// 2025-10-01 09:52:25 UTC.
const TAKEN_AT_SECS: u64 = 1_759_312_345;

fn evidence() -> SteppedInputEvidence {
    SteppedInputEvidence {
        chain_id: "rig:in-a".into(),
        taken_at: SystemTime::UNIX_EPOCH + Duration::from_secs(TAKEN_AT_SECS),
        input_stepped: true,
        streams: vec![StreamEvidence {
            identity: InputStreamIdentity {
                chain_id: "rig:in-a".into(),
                input_index: 0,
                device_id: Some("coreaudio:hd8".into()),
                sample_rate: 44_100,
                buffer_frames: 64,
                channels: 2,
                opened_at: SystemTime::UNIX_EPOCH,
            },
            snapshot: InputEvidenceSnapshot {
                first_frame: 128,
                samples: vec![0.5, -0.5, 0.25, -0.25],
                cycles: vec![
                    CycleRecord {
                        host_ns: 1_000,
                        first_frame: 128,
                        frames: 1,
                    },
                    CycleRecord {
                        host_ns: 2_451,
                        first_frame: 129,
                        frames: 1,
                    },
                ],
            },
        }],
        routes: vec![],
    }
}

fn probe(rate: f64) -> impl Fn(&str) -> DeviceProbe {
    move |device_id| DeviceProbe {
        device_id: device_id.to_string(),
        found: true,
        nominal_rate: Some(rate),
        ..Default::default()
    }
}

fn json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn a_mark_is_a_folder_named_by_time_and_chain() {
    let root = tempfile::tempdir().unwrap();

    let dir = write_mark(root.path(), &evidence(), &probe(44_100.0)).unwrap();

    assert_eq!(dir, root.path().join("20251001-095225-rig_in-a"));
}

#[test]
fn the_mark_keeps_the_raw_input_as_the_hal_delivered_it() {
    let root = tempfile::tempdir().unwrap();

    let dir = write_mark(root.path(), &evidence(), &probe(44_100.0)).unwrap();

    let mut wav = hound::WavReader::open(dir.join("input-0.wav")).unwrap();
    let spec = wav.spec();
    assert_eq!(spec.channels, 2);
    assert_eq!(spec.sample_rate, 44_100);
    assert_eq!(spec.sample_format, hound::SampleFormat::Float);
    let samples: Vec<f32> = wav.samples::<f32>().map(Result::unwrap).collect();
    assert_eq!(samples, vec![0.5, -0.5, 0.25, -0.25]);
}

#[test]
fn the_mark_keeps_the_timing_of_every_cycle() {
    let root = tempfile::tempdir().unwrap();

    let dir = write_mark(root.path(), &evidence(), &probe(44_100.0)).unwrap();

    let csv = std::fs::read_to_string(dir.join("cycles-0.csv")).unwrap();
    assert_eq!(csv, "host_ns,first_frame,frames\n1000,128,1\n2451,129,1\n");
}

#[test]
fn the_mark_keeps_the_device_state_at_the_trip() {
    let root = tempfile::tempdir().unwrap();

    let dir = write_mark(root.path(), &evidence(), &probe(48_000.0)).unwrap();

    let device = json(&dir.join("device-0.json"));
    assert_eq!(device["device_id"], "coreaudio:hd8");
    assert_eq!(device["nominal_rate"], 48_000.0);
}

#[test]
fn the_mark_keeps_what_openrig_had_open() {
    let root = tempfile::tempdir().unwrap();

    let dir = write_mark(root.path(), &evidence(), &probe(44_100.0)).unwrap();

    let openrig = json(&dir.join("openrig.json"));
    assert_eq!(openrig["chain_id"], "rig:in-a");
    assert_eq!(openrig["input_stepped"], true);
    assert_eq!(openrig["streams"][0]["sample_rate"], 44_100);
    assert_eq!(openrig["streams"][0]["first_frame"], 128);
    assert_eq!(openrig["app_version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn the_device_is_read_again_after_the_restart() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_mark(root.path(), &evidence(), &probe(48_000.0)).unwrap();

    write_after_probe(&dir, &evidence(), &probe(44_100.0)).unwrap();

    let after = json(&dir.join("device-after-0.json"));
    assert_eq!(after["nominal_rate"], 44_100.0);
    assert_eq!(json(&dir.join("device-0.json"))["nominal_rate"], 48_000.0);
}

#[test]
fn only_the_newest_marks_are_kept() {
    let root = tempfile::tempdir().unwrap();
    for name in [
        "20251001-095225-rig_in-a",
        "20251001-095301-rig_in-b",
        "20251001-100000-rig_in-a",
    ] {
        std::fs::create_dir(root.path().join(name)).unwrap();
    }

    prune_marks(root.path(), 2).unwrap();

    let mut left: Vec<String> = std::fs::read_dir(root.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(
        left,
        vec!["20251001-095301-rig_in-b", "20251001-100000-rig_in-a"]
    );
}
