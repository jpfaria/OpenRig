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

    store.play_all(&cid());

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
    store.play_all(&cid());

    store.set_enabled(&cid(), 2, true);
    store.play_all(&cid());

    assert_eq!(
        store.status(&cid(), 2).expect("looper exists").state,
        LooperState::Playing,
        "re-enabling lets the next play reach it"
    );
}

/// The transport asks about loopers it may not hold — a uid that was removed,
/// a chain that never had one. Both answers must be safe defaults, not a
/// panic: enabled (nothing to sit out) and unity (nothing to attenuate).
#[test]
fn an_unknown_looper_answers_with_safe_defaults() {
    let store = LooperStore::default();

    assert!(
        store.is_enabled(&cid(), 99),
        "a looper the store does not hold is not 'switched off'"
    );
    assert_eq!(
        store.playback_gain(&cid(), 99),
        1.0,
        "and it is not attenuated either"
    );
}

/// Switching an unknown uid is a no-op — the panel can ask for a looper the
/// store has already dropped.
#[test]
fn switching_an_unknown_looper_changes_nothing() {
    let mut store = two_recorded_loops();

    store.set_enabled(&cid(), 99, false);

    assert!(store.is_enabled(&cid(), 1));
    assert!(store.is_enabled(&cid(), 2));
}
