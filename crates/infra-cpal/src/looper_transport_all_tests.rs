//! #903 — play and stop drive every loop on the chain at once.
//!
//! Loops on one chain are a performance, not independent tape decks: the store
//! already quantizes a new loop to the master's length and restarts everything
//! at 0 when it closes, so they play locked to the same bar. The transport has
//! to match — starting one loop while the others sit stopped drops them out of
//! that bar, and the owner has to press play on each one to get the take back.
//!
//! Two loopers are left alone: an EMPTY one (nothing to play) and one that is
//! RECORDING or overdubbing (its take is still being made — play/stop would cut
//! it short).

use super::looper_store::LooperStore;
use domain::ids::ChainId;
use engine::LooperState;

fn cid() -> ChainId {
    ChainId("transport-all".into())
}

fn record_and_stop(store: &mut LooperStore, chain: &ChainId, uid: u64) {
    store.create(chain, uid);
    store.tap_record(chain, uid);
    store.record_frames(chain, uid, &vec![0.3f32; 100 * 2]);
    store.tap_record(chain, uid); // close → Playing
    store.stop(chain, uid);
}

/// A chain holding two closed loops, both stopped.
fn two_recorded_loops() -> LooperStore {
    let mut store = LooperStore::default();
    store.set_sample_rate(48_000);
    record_and_stop(&mut store, &cid(), 1);
    record_and_stop(&mut store, &cid(), 2);
    store
}

#[test]
fn play_starts_every_recorded_loop_on_the_chain() {
    let mut store = two_recorded_loops();

    store.play(&cid(), 1);

    for uid in [1u64, 2] {
        assert_eq!(
            store.status(&cid(), uid).expect("looper exists").state,
            LooperState::Playing,
            "play must start looper {uid} too — the loops share one bar"
        );
    }
}

#[test]
fn stop_stops_every_playing_loop_on_the_chain() {
    let mut store = two_recorded_loops();
    store.play(&cid(), 1);

    store.stop(&cid(), 2);

    for uid in [1u64, 2] {
        assert_eq!(
            store.status(&cid(), uid).expect("looper exists").state,
            LooperState::Stopped,
            "stop must stop looper {uid} too"
        );
    }
}

#[test]
fn an_empty_looper_is_left_alone() {
    let mut store = two_recorded_loops();
    store.create(&cid(), 3); // never recorded

    store.play(&cid(), 1);

    assert_eq!(
        store.status(&cid(), 3).expect("looper exists").state,
        LooperState::Empty,
        "a looper with no take has nothing to play"
    );
}

#[test]
fn a_recording_looper_keeps_recording() {
    let mut store = two_recorded_loops();
    store.create(&cid(), 3);
    store.tap_record(&cid(), 3); // still taking it
    store.record_frames(&cid(), 3, &vec![0.4f32; 50 * 2]);

    store.play(&cid(), 1);
    let while_playing = store.status(&cid(), 3).expect("looper exists").state;
    store.stop(&cid(), 1);

    assert_eq!(
        while_playing,
        LooperState::Recording,
        "play must not cut a take that is still being recorded"
    );
    assert_eq!(
        store.status(&cid(), 3).expect("looper exists").state,
        LooperState::Recording,
        "stop must not cut it either"
    );
}

/// The chain next door keeps its own transport — one chain never reaches into
/// another (`CLAUDE.md` isolation LAW).
#[test]
fn the_transport_stops_at_the_chains_edge() {
    let mut store = two_recorded_loops();
    let other = ChainId("another-chain".into());
    record_and_stop(&mut store, &other, 1);

    store.play(&cid(), 1);

    assert_eq!(
        store.status(&other, 1).expect("looper exists").state,
        LooperState::Stopped,
        "playing one chain's loops must not touch another chain's"
    );
}
