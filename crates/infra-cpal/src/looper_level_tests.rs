//! #903 — the loop's level is playback gain, not a re-render.
//!
//! `mix` was baked into the exported mixdown, so moving the level knob changed
//! the CONTENT key: the isolated render was rebuilt and the loop restarted from
//! the top, while the level the owner just set only reached the ear once that
//! render took over. Two symptoms, one cause — "quando altero o volume/level o
//! loop reinicia" and "quando altero o level não abaixa o volume".
//!
//! Level belongs to playback: the take on disk and in the store is the take.

use super::looper_store::LooperStore;
use domain::ids::ChainId;

fn cid() -> ChainId {
    ChainId("level".into())
}

fn loaded_loop() -> LooperStore {
    let mut store = LooperStore::default();
    store.set_sample_rate(48_000);
    store.create(&cid(), 1);
    store.load(&cid(), 1, &vec![0.5f32; 4_800 * 2]);
    store.play(&cid(), 1);
    store
}

#[test]
fn moving_the_level_does_not_restart_the_loop() {
    let mut store = loaded_loop();
    let before = store.status(&cid(), 1).expect("looper").content_rev;

    store.set_mix(&cid(), 1, 0.25);

    assert_eq!(
        store.status(&cid(), 1).expect("looper").content_rev,
        before,
        "the level is not content — bumping it re-renders the loop and restarts it"
    );
}

#[test]
fn the_level_is_the_gain_the_playback_reads() {
    let mut store = loaded_loop();

    store.set_mix(&cid(), 1, 0.25);

    assert!(
        (store.playback_gain(&cid(), 1) - 0.25).abs() < 1e-6,
        "the level the owner set must be the gain the stream applies, right away"
    );
}

#[test]
fn the_take_itself_is_never_scaled_by_the_level() {
    let mut store = loaded_loop();

    store.set_mix(&cid(), 1, 0.25);
    let pcm = store.export(&cid(), 1).expect("the take");

    let peak = pcm.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(
        (peak - 0.5).abs() < 1e-3,
        "the exported take stays as recorded (peak {peak:.3}) — level is applied \
         on playback, so lowering it never damages the material"
    );
}
