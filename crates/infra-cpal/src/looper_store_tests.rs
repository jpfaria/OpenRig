//! #323 — the controller-owned looper store drives the whole loop lifecycle
//! with NO chain runtime in sight — the property the old bank-in-runtime design
//! could not hold (stop/clear only landed inside a running audio callback).

use super::*;
use engine::LooperState;

fn cid() -> ChainId {
    ChainId("c".into())
}

#[test]
fn record_close_play_stop_clear_are_deterministic_without_any_runtime() {
    let mut store = LooperStore::default();
    store.create(&cid(), 1);
    assert_eq!(store.status(&cid(), 1).unwrap().state, LooperState::Empty);

    // Record: start, feed 3 stereo frames of dry audio, close.
    store.tap_record(&cid(), 1);
    assert_eq!(store.status(&cid(), 1).unwrap().state, LooperState::Recording);
    store.record_frames(&cid(), 1, &[0.2, 0.2, 0.3, 0.3, 0.4, 0.4]);
    store.tap_record(&cid(), 1); // close → Playing
    let s = store.status(&cid(), 1).unwrap();
    assert_eq!(s.state, LooperState::Playing);
    assert_eq!(s.len_frames, 3);
    assert!(store.export(&cid(), 1).is_some(), "a closed loop exports audio");

    // Stop / clear act with NO runtime — the whole point of the redesign.
    store.stop(&cid(), 1);
    assert_eq!(store.status(&cid(), 1).unwrap().state, LooperState::Stopped);
    store.clear(&cid(), 1);
    let s = store.status(&cid(), 1).unwrap();
    assert_eq!(s.state, LooperState::Empty);
    assert_eq!(s.len_frames, 0);

    store.remove(&cid(), 1);
    assert!(store.status(&cid(), 1).is_none());
}

#[test]
fn drain_recording_captures_dry_samples_from_the_tap_ring() {
    let mut store = LooperStore::default();
    store.create(&cid(), 1);
    store.tap_record(&cid(), 1); // start recording
    assert!(!store.is_recording_armed(&cid(), 1), "no rings yet");

    let ring = std::sync::Arc::new(engine::spsc::SpscRing::<f32>::new(64, 0.0));
    for s in [0.1f32, 0.2, 0.3] {
        ring.push(s);
    }
    store.set_recording_rings(&cid(), 1, vec![ring]);
    assert!(store.is_recording_armed(&cid(), 1));

    store.drain_recording(&cid(), 1); // pull the 3 samples in
    store.tap_record(&cid(), 1); // close → Playing

    let st = store.status(&cid(), 1).unwrap();
    assert_eq!(st.state, LooperState::Playing);
    assert_eq!(st.len_frames, 3, "3 mono samples became 3 stereo frames");
    assert!(
        !store.is_recording_armed(&cid(), 1),
        "closing the recording drops the rings"
    );
}

#[test]
fn each_loop_is_isolated_and_carries_its_routing() {
    let mut store = LooperStore::default();
    store.create(&cid(), 7);
    store.set_output(
        &cid(),
        7,
        Some(EndpointRef {
            binding_id: "io".into(),
            endpoint: "out1".into(),
        }),
    );
    assert_eq!(store.output(&cid(), 7).unwrap().endpoint, "out1");
    // A different uid is untouched.
    store.create(&cid(), 8);
    assert!(store.output(&cid(), 8).is_none());
    assert_eq!(store.statuses(&cid()).len(), 2);
}
