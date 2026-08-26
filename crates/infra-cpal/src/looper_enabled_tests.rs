//! #903 — a disabled looper sits out the chain's transport.
//!
//! Play and stop move every loop on the chain (one performance, one bar), so
//! the only way to keep a take out of the take is to switch that looper off.
//! Disabling is not stopping: a disabled looper keeps its recording, it just
//! stops answering the transport until it is switched back on.

use super::looper_store::LooperStore;
use domain::ids::ChainId;
use engine::LooperState;

fn cid() -> ChainId {
    ChainId("enabled".into())
}

fn record_and_stop(store: &mut LooperStore, uid: u64) {
    store.create(&cid(), uid);
    store.tap_record(&cid(), uid);
    store.record_frames(&cid(), uid, &vec![0.3f32; 100 * 2]);
    store.tap_record(&cid(), uid);
    store.stop(&cid(), uid);
}

fn two_recorded_loops() -> LooperStore {
    let mut store = LooperStore::default();
    store.set_sample_rate(48_000);
    record_and_stop(&mut store, 1);
    record_and_stop(&mut store, 2);
    store
}

#[test]
fn play_skips_a_disabled_looper() {
    let mut store = two_recorded_loops();
    store.set_enabled(&cid(), 2, false);

    store.play(&cid(), 1);

    assert_eq!(
        store.status(&cid(), 1).expect("looper exists").state,
        LooperState::Playing,
        "the enabled looper plays"
    );
    assert_eq!(
        store.status(&cid(), 2).expect("looper exists").state,
        LooperState::Stopped,
        "a disabled looper must sit the take out"
    );
}

#[test]
fn a_disabled_looper_keeps_its_take() {
    let mut store = two_recorded_loops();

    store.set_enabled(&cid(), 2, false);

    assert!(
        store.status(&cid(), 2).expect("looper exists").len_frames > 0,
        "disabling is not clearing — the recording stays"
    );
}

#[test]
fn switching_it_back_on_puts_it_back_in_the_take() {
    let mut store = two_recorded_loops();
    store.set_enabled(&cid(), 2, false);
    store.play(&cid(), 1);

    store.set_enabled(&cid(), 2, true);
    store.play(&cid(), 1);

    assert_eq!(
        store.status(&cid(), 2).expect("looper exists").state,
        LooperState::Playing,
        "re-enabling lets the next play reach it"
    );
}
