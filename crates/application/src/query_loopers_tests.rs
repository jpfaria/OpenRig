//! #323 — the looper read model every transport shares.

use super::*;
use project::chain::{Chain, LooperConfig, LooperSpeed};

fn chain() -> Chain {
    Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![LooperConfig {
            uid: 1,
            mix: 0.8,
            decay: 0.5,
            speed: LooperSpeed::Double,
            reverse: true,
            audio_file: None,
        }],
    }
}

fn status(uid: u64) -> engine::LooperStatus {
    engine::LooperStatus {
        uid,
        state: engine::LooperState::Playing,
        position_frames: 512,
        len_frames: 48_000,
        layers: 3,
    }
}

#[test]
fn json_merges_persisted_params_with_live_transport_state() {
    let json: serde_json::Value =
        serde_json::from_str(&loopers_json(&chain(), &[status(1)], 48_000)).expect("valid json");

    assert_eq!(json["chain"], "c1");
    let l = &json["loopers"][0];
    assert_eq!(l["uid"], 1);
    assert_eq!(l["state"], "playing");
    assert_eq!(l["position_frames"], 512);
    assert_eq!(l["len_frames"], 48_000);
    assert_eq!(l["length_seconds"], 1.0, "seconds come from the LIVE rate");
    assert_eq!(l["layers"], 3);
    // f32 params widen to f64 in JSON, so compare with a tolerance.
    assert!((l["mix"].as_f64().unwrap() - 0.8).abs() < 1e-6);
    assert_eq!(l["decay"], 0.5);
    assert_eq!(l["speed"], "double");
    assert_eq!(l["reverse"], true);
}

#[test]
fn a_looper_with_no_runtime_yet_reads_as_empty() {
    let json: serde_json::Value =
        serde_json::from_str(&loopers_json(&chain(), &[], 48_000)).expect("valid json");

    let l = &json["loopers"][0];
    assert_eq!(l["state"], "empty");
    assert_eq!(l["len_frames"], 0);
    assert_eq!(l["layers"], 0);
}

#[test]
fn seconds_follow_the_live_sample_rate_never_a_hardcoded_48000() {
    let json: serde_json::Value =
        serde_json::from_str(&loopers_json(&chain(), &[status(1)], 44_100)).expect("valid json");
    let seconds = json["loopers"][0]["length_seconds"].as_f64().unwrap();
    assert!(
        (seconds - 48_000.0 / 44_100.0).abs() < 1e-6,
        "length must be derived from the stream's real rate, got {seconds}"
    );
}
