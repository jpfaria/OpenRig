//! A waveform edit on one loop is carried to every loop on the shared
//! timeline, so the loops still line up after it.

use super::*;
use engine::loop_edit::{LoopEditOp, SEAM_FRAMES};
use engine::LooperState;

fn chain(name: &str) -> ChainId {
    ChainId(name.into())
}

/// Frame `i` of loop `tag` is `[tag * 100_000 + i, -(…)]`: every frame names
/// its loop and its position, exactly representable in `f32`.
fn take(tag: usize, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let v = (tag * 100_000 + i) as f32;
            [v, -v]
        })
        .collect()
}

/// A stopped loop holding `pcm`, the way a project reopen installs it.
fn install(store: &mut LooperStore, c: &ChainId, uid: u64, pcm: &[f32]) {
    store.create(c, uid);
    store.load(c, uid, pcm);
}

fn frame(pcm: &[f32], i: usize) -> [f32; 2] {
    [pcm[i * 2], pcm[i * 2 + 1]]
}

fn raw(store: &LooperStore, c: &ChainId, uid: u64) -> Vec<f32> {
    store.export_raw(c, uid).expect("the loop holds material")
}

fn len(store: &LooperStore, c: &ChainId, uid: u64) -> usize {
    store.status(c, uid).expect("the loop exists").len_frames
}

#[test]
fn trimming_a_loop_trims_the_same_region_from_a_loop_of_the_same_length() {
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    let b = take(2, 1024);
    install(&mut store, &c, 1, &take(1, 1024));
    install(&mut store, &c, 2, &b);

    let new_len = store
        .apply_edit(&c, 1, LoopEditOp::Keep, 100, 900)
        .expect("a stopped loop can be trimmed");

    assert_eq!(
        len(&store, &c, 2),
        new_len,
        "the other loop ends up exactly as long as the edited one"
    );
    let edited = raw(&store, &c, 2);
    // Past the seam, frame p is the source frame the edited loop kept there.
    for p in [SEAM_FRAMES, 300, new_len - 1] {
        assert_eq!(frame(&edited, p), frame(&b, 100 + p), "frame {p}");
    }
}

#[test]
fn an_edit_reaches_the_loops_of_other_chains() {
    // The timeline is the whole project's, not one chain's.
    let mut store = LooperStore::default();
    install(&mut store, &chain("guitar"), 1, &take(1, 1024));
    install(&mut store, &chain("acoustic"), 1, &take(2, 1024));

    let new_len = store
        .apply_edit(&chain("guitar"), 1, LoopEditOp::Keep, 100, 900)
        .unwrap();

    assert_eq!(len(&store, &chain("acoustic"), 1), new_len);
}

#[test]
fn a_loop_two_cycles_long_loses_the_region_from_each_cycle() {
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    let b = take(2, 2048);
    install(&mut store, &c, 1, &take(1, 1024));
    install(&mut store, &c, 2, &b);

    // Cut [400, 600): head 0..400 joins tail 600..1024 → 696 frames a cycle.
    let cycle = store.apply_edit(&c, 1, LoopEditOp::Cut, 400, 600).unwrap();

    assert_eq!(len(&store, &c, 2), 2 * cycle, "still two cycles long");
    let edited = raw(&store, &c, 2);
    // Before the cut each cycle is its own source; after it, 200 cut frames
    // plus the 64-frame join later.
    assert_eq!(frame(&edited, 100), frame(&b, 100));
    assert_eq!(frame(&edited, cycle + 100), frame(&b, 1024 + 100));
    assert_eq!(frame(&edited, cycle + 500), frame(&b, 1024 + 764));
}

#[test]
fn a_loop_shorter_than_the_edited_one_repeats_under_it_before_the_edit() {
    // Editing a two-cycle loop: the one-cycle loop plays twice under it, so
    // the region is taken out of those two passes and both stay together.
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    let b = take(2, 1024);
    install(&mut store, &c, 1, &take(1, 2048));
    install(&mut store, &c, 2, &b);

    let new_len = store
        .apply_edit(&c, 1, LoopEditOp::Keep, 100, 1900)
        .unwrap();

    assert_eq!(len(&store, &c, 2), new_len);
    let edited = raw(&store, &c, 2);
    assert_eq!(frame(&edited, SEAM_FRAMES), frame(&b, 100 + SEAM_FRAMES));
    assert_eq!(frame(&edited, 1500), frame(&b, (100 + 1500) % 1024));
}

#[test]
fn a_loop_off_the_timeline_is_left_alone() {
    // Neither length divides the other: there is no region of it that lines
    // up with the edit, so it is not touched.
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    let b = take(2, 1500);
    install(&mut store, &c, 1, &take(1, 1000));
    install(&mut store, &c, 2, &b);

    store.apply_edit(&c, 1, LoopEditOp::Keep, 100, 900).unwrap();

    assert_eq!(raw(&store, &c, 2), b);
    assert_eq!(store.edit_history_depth(&c, 2), (0, 0));
}

#[test]
fn a_fit_carries_the_bounds_it_found_on_the_edited_loop() {
    // FIT finds its region on the loop it was pressed on; the others get that
    // same region, not one worked out from their own audio.
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    let mut a = vec![0.0f32; 4000 * 2];
    for f in 800..3000 {
        a[f * 2] = 0.5;
        a[f * 2 + 1] = -0.5;
    }
    let b = take(2, 4000);
    install(&mut store, &c, 1, &a);
    install(&mut store, &c, 2, &b);
    let (start, _) = engine::loop_edit::content_bounds(&a).unwrap();

    let new_len = store.apply_edit(&c, 1, LoopEditOp::Fit, 0, 0).unwrap();

    assert_eq!(len(&store, &c, 2), new_len);
    assert_eq!(
        frame(&raw(&store, &c, 2), SEAM_FRAMES),
        frame(&b, start + SEAM_FRAMES)
    );
}

/// A 2 s phrase of four held notes, played 2.6 times between half a second of
/// silence on each side, at the store's default 48 kHz.
fn phrase_take() -> Vec<f32> {
    let r = 48_000.0;
    let (lead, played) = (24_000, 249_600);
    let notes = [196.0, 247.0, 294.0, 330.0];
    let mut pcm = vec![0.0f32; (lead + played + lead) * 2];
    for i in 0..played {
        let t = i as f64 / r;
        let s = 0.3 * (std::f64::consts::TAU * notes[(t / 0.5) as usize % 4] * t).sin();
        pcm[(lead + i) * 2] = s as f32;
        pcm[(lead + i) * 2 + 1] = (s * 0.5) as f32;
    }
    pcm
}

#[test]
fn a_fit_on_a_repeating_phrase_leaves_whole_passes_of_it() {
    // The store hands FIT its sample rate: 2.6 passes of a 2 s phrase come
    // out exactly two passes long, not the 5.2 s that were played.
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    install(&mut store, &c, 1, &phrase_take());

    let new_len = store.apply_edit(&c, 1, LoopEditOp::Fit, 0, 0).unwrap();

    assert!(
        (new_len as i64 - 192_000).abs() < 240,
        "two passes are 192000 frames, the loop is {new_len}"
    );
}

#[test]
fn undo_of_a_carried_edit_restores_every_loop_it_touched() {
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    let (a, b) = (take(1, 1024), take(2, 1024));
    install(&mut store, &c, 1, &a);
    install(&mut store, &c, 2, &b);
    store.apply_edit(&c, 1, LoopEditOp::Keep, 100, 900).unwrap();
    let b_edited = raw(&store, &c, 2);

    assert!(store.undo_edit(&c, 1));
    assert_eq!(raw(&store, &c, 1), a);
    assert_eq!(raw(&store, &c, 2), b, "one edit, one undo, for every loop");

    assert!(store.redo_edit(&c, 1));
    assert_eq!(raw(&store, &c, 2), b_edited);
}

#[test]
fn undo_pressed_on_another_loop_undoes_the_same_edit() {
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    let (a, b) = (take(1, 1024), take(2, 1024));
    install(&mut store, &c, 1, &a);
    install(&mut store, &c, 2, &b);
    store.apply_edit(&c, 1, LoopEditOp::Keep, 100, 900).unwrap();

    assert!(store.undo_edit(&c, 2));

    assert_eq!(raw(&store, &c, 1), a);
    assert_eq!(raw(&store, &c, 2), b);
}

#[test]
fn one_undo_steps_back_one_edit_on_every_loop() {
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    install(&mut store, &c, 1, &take(1, 2048));
    install(&mut store, &c, 2, &take(2, 2048));
    store
        .apply_edit(&c, 1, LoopEditOp::Keep, 100, 1900)
        .unwrap();
    let after_first = (raw(&store, &c, 1), raw(&store, &c, 2));
    store.apply_edit(&c, 1, LoopEditOp::Cut, 400, 600).unwrap();

    assert!(store.undo_edit(&c, 1));

    assert_eq!((raw(&store, &c, 1), raw(&store, &c, 2)), after_first);
}

#[test]
fn a_playing_loop_is_reshaped_and_lands_stopped() {
    // Like the loop under the pencil, a reshaped loop comes back stopped: it
    // is never swapped under the player's feet mid-lap.
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    install(&mut store, &c, 1, &take(1, 1024));
    install(&mut store, &c, 2, &take(2, 1024));
    store.play(&c, 2);

    let new_len = store.apply_edit(&c, 1, LoopEditOp::Keep, 100, 900).unwrap();

    let other = store.status(&c, 2).unwrap();
    assert_eq!(other.len_frames, new_len);
    assert_eq!(other.state, LooperState::Stopped);
}

#[test]
fn a_take_being_recorded_is_not_touched() {
    let mut store = LooperStore::default();
    let c = chain("acoustic");
    install(&mut store, &c, 1, &take(1, 1024));
    store.create(&c, 2);
    store.tap_record(&c, 2);
    store.record_frames(&c, 2, &take(2, 300));

    store.apply_edit(&c, 1, LoopEditOp::Keep, 100, 900).unwrap();

    let recording = store.status(&c, 2).unwrap();
    assert_eq!(recording.state, LooperState::Recording);
    assert_eq!(recording.len_frames, 300);
}
