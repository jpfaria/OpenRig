//! #903 — two scopes: this loop, or the whole chain.
//!
//! The owner wants both, and they are different buttons: a row's play is for
//! hearing ONE loop, and the panel's global play starts everything at once.
//! An earlier round made the row's play move the whole chain, which took the
//! first away.
//!
//! The chain-wide scope skips what it cannot start: a looper with no take, one
//! still recording (its take would be cut short), and one switched off.

use super::looper_store::LooperStore;
use domain::ids::ChainId;
use engine::LooperState;

fn cid() -> ChainId {
    ChainId("scope".into())
}

fn loaded(store: &mut LooperStore, chain: &ChainId, uid: u64) {
    store.create(chain, uid);
    store.load(chain, uid, &vec![0.3f32; 100 * 2]);
}

fn two_recorded_loops() -> LooperStore {
    let mut store = LooperStore::default();
    store.set_sample_rate(48_000);
    loaded(&mut store, &cid(), 1);
    loaded(&mut store, &cid(), 2);
    store
}

fn state(store: &LooperStore, uid: u64) -> LooperState {
    store.status(&cid(), uid).expect("looper exists").state
}

#[test]
fn a_rows_play_starts_only_that_loop() {
    let mut store = two_recorded_loops();

    store.play(&cid(), 1);

    assert_eq!(state(&store, 1), LooperState::Playing);
    assert_eq!(
        state(&store, 2),
        LooperState::Stopped,
        "the row's play is for hearing ONE loop"
    );
}

#[test]
fn a_rows_stop_stops_only_that_loop() {
    let mut store = two_recorded_loops();
    store.play_all(&cid());

    store.stop(&cid(), 1);

    assert_eq!(state(&store, 1), LooperState::Stopped);
    assert_eq!(
        state(&store, 2),
        LooperState::Playing,
        "stopping one loop leaves the others playing"
    );
}

#[test]
fn the_global_play_starts_every_loop_on_the_chain() {
    let mut store = two_recorded_loops();

    store.play_all(&cid());

    assert_eq!(state(&store, 1), LooperState::Playing);
    assert_eq!(state(&store, 2), LooperState::Playing);
}

#[test]
fn the_global_stop_stops_every_loop_on_the_chain() {
    let mut store = two_recorded_loops();
    store.play_all(&cid());

    store.stop_all(&cid());

    assert_eq!(state(&store, 1), LooperState::Stopped);
    assert_eq!(state(&store, 2), LooperState::Stopped);
}

#[test]
fn the_global_play_skips_a_disabled_looper() {
    let mut store = two_recorded_loops();
    store.set_enabled(&cid(), 2, false);

    store.play_all(&cid());

    assert_eq!(state(&store, 1), LooperState::Playing);
    assert_eq!(
        state(&store, 2),
        LooperState::Stopped,
        "a looper switched off sits the take out"
    );
}

#[test]
fn the_global_play_leaves_a_recording_looper_alone() {
    let mut store = two_recorded_loops();
    store.create(&cid(), 3);
    store.tap_record(&cid(), 3);
    store.record_frames(&cid(), 3, &vec![0.4f32; 50 * 2]);

    store.play_all(&cid());

    assert_eq!(
        state(&store, 3),
        LooperState::Recording,
        "the take being made is not interrupted"
    );
}

#[test]
fn the_global_scope_stops_at_the_chains_edge() {
    let mut store = two_recorded_loops();
    let other = ChainId("another".into());
    loaded(&mut store, &other, 1);

    store.play_all(&cid());

    assert_eq!(
        store.status(&other, 1).expect("looper").state,
        LooperState::Stopped,
        "one chain's global play never reaches another chain"
    );
}
