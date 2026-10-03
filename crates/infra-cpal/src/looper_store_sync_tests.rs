//! Every looper of the project, on every chain, follows ONE timeline: the
//! first take sets the cycle and the anchor, every later take is a whole
//! number of cycles long and stored so its first frame is the top of the
//! cycle, and starting from silence puts every loop at the top together.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use domain::ids::ChainId;
use engine::LooperState;

use crate::loop_sync::PLAY_LEAD_NS;
use crate::looper_store::LooperStore;

const MS: u64 = 1_000_000;
const S: u64 = 1_000 * MS;
/// One frame per millisecond keeps the timeline readable.
const RATE: u32 = 1_000;

fn chain(name: &str) -> ChainId {
    ChainId(name.into())
}

/// A store on a clock the test moves by hand.
fn store() -> (LooperStore, Arc<AtomicU64>) {
    let now = Arc::new(AtomicU64::new(0));
    let mut store = LooperStore::default();
    store.set_sample_rate(RATE);
    let clock = Arc::clone(&now);
    store.set_clock(Arc::new(move || clock.load(Ordering::Relaxed)));
    (store, now)
}

/// Mono frames whose value is `first + i`, as the input tap delivers them.
fn frames(first: usize, n: usize) -> Vec<f32> {
    (first..first + n)
        .flat_map(|v| [v as f32, v as f32])
        .collect()
}

/// Record one take on `(c, uid)`: REC at `open`, the first sample captured at
/// `first_capture`, REC again at `close`, then `captured` frames delivered.
fn take(
    store: &mut LooperStore,
    now: &AtomicU64,
    c: &ChainId,
    uid: u64,
    (open, first_capture, close): (u64, u64, u64),
    captured: usize,
) {
    store.create(c, uid);
    now.store(open, Ordering::Relaxed);
    store.tap_record(c, uid);
    store.set_recording_stamp(c, uid, Arc::new(AtomicU64::new(first_capture)));
    now.store(close, Ordering::Relaxed);
    store.tap_record(c, uid);
    store.record_frames(c, uid, &frames(1, captured));
}

fn left(store: &LooperStore, c: &ChainId, uid: u64, frame: usize) -> f32 {
    store.export_raw(c, uid).unwrap()[frame * 2]
}

#[test]
fn the_first_take_lasts_exactly_the_time_between_the_two_presses() {
    let (mut store, now) = store();
    let a = chain("a");
    // REC at 1 s, REC at 3 s: a 2000-frame loop, whatever arrived late.
    take(&mut store, &now, &a, 1, (S, S + 50 * MS, 3 * S), 2_000);

    let status = store.status(&a, 1).unwrap();
    assert_eq!(status.state, LooperState::Playing);
    assert_eq!(status.len_frames, 2_000);
    assert_eq!(store.sync_anchor(), Some(S), "the first REC is the top");
}

#[test]
fn the_first_take_keeps_recording_until_its_length_is_captured() {
    let (mut store, now) = store();
    let a = chain("a");
    take(&mut store, &now, &a, 1, (S, S + 50 * MS, 3 * S), 1_950);
    assert_eq!(
        store.status(&a, 1).unwrap().state,
        LooperState::Recording,
        "the samples captured before the second REC are still on their way"
    );

    store.record_frames(&a, 1, &frames(1_951, 80));
    let status = store.status(&a, 1).unwrap();
    assert_eq!(status.state, LooperState::Playing);
    assert_eq!(
        status.len_frames, 2_000,
        "the extra frames are not the take"
    );
}

#[test]
fn the_first_take_starts_at_the_first_rec_even_when_its_first_sample_came_later() {
    let (mut store, now) = store();
    let a = chain("a");
    // The first sample was captured 50 ms after the REC: it is frame 50 of the
    // loop, and the 50 frames played right after the closing REC wrap to the
    // top — the downbeat the player hit when the loop came round.
    take(&mut store, &now, &a, 1, (S, S + 50 * MS, 3 * S), 2_000);
    assert_eq!(left(&store, &a, 1, 50), 1.0);
    assert_eq!(left(&store, &a, 1, 1_999), 1_950.0);
    assert_eq!(left(&store, &a, 1, 0), 1_951.0);
}

#[test]
fn a_take_on_another_chain_rounds_to_whole_cycles_of_the_first() {
    let (mut store, now) = store();
    let (a, b) = (chain("a"), chain("b"));
    take(&mut store, &now, &a, 1, (S, S, 3 * S), 2_000);

    // 3.7 s of take on chain b → 2 cycles.
    take(
        &mut store,
        &now,
        &b,
        7,
        (3_500 * MS, 3_500 * MS, 7_200 * MS),
        4_000,
    );
    let status = store.status(&b, 7).unwrap();
    assert_eq!(status.state, LooperState::Playing);
    assert_eq!(status.len_frames, 4_000);
}

#[test]
fn a_take_longer_than_whole_cycles_is_trimmed_to_them() {
    let (mut store, now) = store();
    let (a, b) = (chain("a"), chain("b"));
    take(&mut store, &now, &a, 1, (S, S, 3 * S), 2_000);
    // 4.9 s → 2 cycles: the take closes once 4000 frames are in.
    take(&mut store, &now, &b, 7, (4 * S, 4 * S, 8_900 * MS), 4_000);
    let status = store.status(&b, 7).unwrap();
    assert_eq!(status.state, LooperState::Playing);
    assert_eq!(status.len_frames, 4_000);
}

#[test]
fn a_take_shorter_than_half_a_cycle_lasts_one_cycle() {
    let (mut store, now) = store();
    let (a, b) = (chain("a"), chain("b"));
    take(&mut store, &now, &a, 1, (S, S, 3 * S), 2_000);
    take(&mut store, &now, &b, 7, (4 * S, 4 * S, 4_600 * MS), 2_000);
    assert_eq!(store.status(&b, 7).unwrap().len_frames, 2_000);
}

#[test]
fn a_later_take_is_stored_so_its_first_frame_is_the_top_of_the_cycle() {
    let (mut store, now) = store();
    let (a, b) = (chain("a"), chain("b"));
    take(&mut store, &now, &a, 1, (S, S, 3 * S), 2_000);
    // Started 2.5 s after the anchor: its first sample sits 2500 frames into
    // a 4000-frame loop, so a stream starting at the anchor plays it there.
    take(
        &mut store,
        &now,
        &b,
        7,
        (3_500 * MS, 3_500 * MS, 7_200 * MS),
        4_000,
    );
    assert_eq!(left(&store, &b, 7, 2_500), 1.0);
    assert_eq!(left(&store, &b, 7, 0), 1_501.0);
}

#[test]
fn closing_a_take_never_restarts_the_loops_already_playing() {
    let (mut store, now) = store();
    let (a, b) = (chain("a"), chain("b"));
    take(&mut store, &now, &a, 1, (S, S, 3 * S), 2_000);
    let rev = store.status(&a, 1).unwrap().content_rev;
    take(
        &mut store,
        &now,
        &b,
        7,
        (3_500 * MS, 3_500 * MS, 7_200 * MS),
        4_000,
    );
    assert_eq!(
        store.status(&a, 1).unwrap().content_rev,
        rev,
        "the first loop keeps playing: no re-arm, no jump back to the top"
    );
    assert_eq!(store.sync_anchor(), Some(S));
}

#[test]
fn play_from_silence_puts_every_loop_at_the_top_together() {
    let (mut store, now) = store();
    let (a, b) = (chain("a"), chain("b"));
    take(&mut store, &now, &a, 1, (S, S, 3 * S), 2_000);
    take(
        &mut store,
        &now,
        &b,
        7,
        (3_500 * MS, 3_500 * MS, 7_200 * MS),
        4_000,
    );
    store.stop_all(&a);
    store.stop_all(&b);

    now.store(20 * S, Ordering::Relaxed);
    store.play_all(&a);
    assert_eq!(
        store.sync_anchor(),
        Some(20 * S + PLAY_LEAD_NS),
        "nothing was sounding: the timeline restarts one lead ahead"
    );
}

#[test]
fn play_while_another_chain_sounds_joins_its_timeline() {
    let (mut store, now) = store();
    let (a, b) = (chain("a"), chain("b"));
    take(&mut store, &now, &a, 1, (S, S, 3 * S), 2_000);
    take(
        &mut store,
        &now,
        &b,
        7,
        (3_500 * MS, 3_500 * MS, 7_200 * MS),
        4_000,
    );
    store.stop(&b, 7);

    now.store(20 * S, Ordering::Relaxed);
    store.play(&b, 7);
    assert_eq!(store.status(&b, 7).unwrap().state, LooperState::Playing);
    assert_eq!(
        store.sync_anchor(),
        Some(S),
        "chain a is playing: chain b joins in phase, nothing restarts"
    );
}

#[test]
fn a_take_over_silence_starts_a_new_timeline_at_its_rec() {
    let (mut store, now) = store();
    let (a, b) = (chain("a"), chain("b"));
    take(&mut store, &now, &a, 1, (S, S, 3 * S), 2_000);
    store.stop(&a, 1);

    take(
        &mut store,
        &now,
        &b,
        7,
        (30 * S, 30 * S, 33_900 * MS),
        4_000,
    );
    assert_eq!(store.sync_anchor(), Some(30 * S));
    let status = store.status(&b, 7).unwrap();
    assert_eq!(
        status.len_frames, 4_000,
        "still whole cycles of the first take"
    );
    assert_eq!(
        left(&store, &b, 7, 0),
        1.0,
        "nothing to line up with: the take starts at its own top"
    );
}

#[test]
fn stop_while_recording_closes_the_take_on_the_cycle_and_stops_it() {
    let (mut store, now) = store();
    let (a, b) = (chain("a"), chain("b"));
    take(&mut store, &now, &a, 1, (S, S, 3 * S), 2_000);

    store.create(&b, 7);
    now.store(4 * S, Ordering::Relaxed);
    store.tap_record(&b, 7);
    store.set_recording_stamp(&b, 7, Arc::new(AtomicU64::new(4 * S)));
    now.store(7_800 * MS, Ordering::Relaxed);
    store.stop(&b, 7);
    store.record_frames(&b, 7, &frames(1, 4_000));

    let status = store.status(&b, 7).unwrap();
    assert_eq!(status.state, LooperState::Stopped);
    assert_eq!(status.len_frames, 4_000);
}

#[test]
fn a_take_without_a_capture_time_is_what_was_captured() {
    let (mut store, now) = store();
    let a = chain("a");
    store.create(&a, 1);
    now.store(S, Ordering::Relaxed);
    store.tap_record(&a, 1);
    store.record_frames(&a, 1, &frames(1, 300));
    now.store(9 * S, Ordering::Relaxed);
    store.tap_record(&a, 1);

    let status = store.status(&a, 1).unwrap();
    assert_eq!(status.state, LooperState::Playing);
    assert_eq!(status.len_frames, 300);
    assert_eq!(left(&store, &a, 1, 0), 1.0);
}

#[test]
fn a_take_whose_tap_never_stamps_a_capture_time_is_what_it_captured() {
    let (mut store, now) = store();
    let a = chain("a");
    store.create(&a, 1);
    now.store(S, Ordering::Relaxed);
    store.tap_record(&a, 1);
    // A backend with no capture time leaves the tap's cell at 0.
    store.set_recording_stamp(&a, 1, Arc::new(AtomicU64::new(0)));
    store.record_frames(&a, 1, &frames(1, 128));
    // The presses are 1 ms apart, but 128 frames arrived: they are the take.
    now.store(S + MS, Ordering::Relaxed);
    store.tap_record(&a, 1);

    let status = store.status(&a, 1).unwrap();
    assert_eq!(status.state, LooperState::Playing);
    assert_eq!(status.len_frames, 128);
    assert_eq!(left(&store, &a, 1, 0), 1.0, "nothing is rotated");
}
