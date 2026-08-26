//! #903 — a loop that is already playing must not be re-armed by the transport.
//!
//! The isolated render re-arms whenever the loop's CONTENT key moves, and each
//! re-arm spawns a render thread that rebuilds the chain (NAM + IR off disk)
//! and copies the whole take. Measured on the owner's rig while playing: 28
//! threads → 43, 46 MB → 396 MB, 2 % → 140 % CPU, and the live `dsp-worker`
//! logging `17159us wall / 397us cpu` — starved, not busy.
//!
//! So the transport must be idempotent for a loop that is already in that
//! state: pressing play with loops running (or a play tap that now moves the
//! whole chain) may not bump the content revision of a loop that was already
//! playing.

use super::looper_store::LooperStore;
use domain::ids::ChainId;

fn cid() -> ChainId {
    ChainId("churn".into())
}

fn loaded(store: &mut LooperStore, uid: u64) {
    store.create(&cid(), uid);
    store.load(&cid(), uid, &vec![0.3f32; 4_800 * 2]);
}

fn content_revs(store: &LooperStore) -> Vec<u64> {
    [1u64, 2]
        .iter()
        .map(|uid| store.status(&cid(), *uid).expect("looper").content_rev)
        .collect()
}

#[test]
fn playing_again_does_not_move_a_playing_loops_content() {
    let mut store = LooperStore::default();
    store.set_sample_rate(48_000);
    loaded(&mut store, 1);
    loaded(&mut store, 2);
    store.play(&cid(), 1);
    let after_first_play = content_revs(&store);

    // The owner taps play again — with the chain-wide transport this reaches
    // both loops, and both are already playing.
    store.play(&cid(), 1);

    assert_eq!(
        content_revs(&store),
        after_first_play,
        "a loop that is already playing must keep its content revision — every \
         bump respawns its render thread and copies the take again"
    );
}
