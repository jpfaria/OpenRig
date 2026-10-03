//! A playback that starts at a host-clock instant: silent before it, exact
//! inside the buffer that contains it, and caught up when it is late — so
//! every loop on the shared timeline sounds at the same loop position.

use super::*;

const MS: u64 = 1_000_000;
/// One frame per millisecond keeps the arithmetic readable.
const RATE: u32 = 1_000;

fn timed_cell(start_pos: usize, start_at_ns: u64) -> (DiPlaybackCell, Arc<DiPlayback>) {
    let playback =
        Arc::new(DiPlayback::starting_at(0, 1, 2_000, start_pos).at_host_time(start_at_ns, RATE));
    (
        Arc::new(ArcSwapOption::from(Some(Arc::clone(&playback)))),
        playback,
    )
}

/// The sample marking frame `i` (+1, so 0 means "silent"), kept well under
/// the output limiter's threshold so it comes out untouched.
fn marker(i: usize) -> f32 {
    (i + 1) as f32 / 100.0
}

/// Push `n` frames whose left sample marks their index.
fn fill(playback: &DiPlayback, n: usize) {
    let ring = playback.ring();
    for i in 0..n {
        assert!(ring.push(marker(i)));
        assert!(ring.push(-marker(i)));
    }
}

/// The frame number each left sample marks.
fn lefts(out: &[f32]) -> Vec<f32> {
    out.chunks(2).map(|f| (f[0] * 100.0).round()).collect()
}

#[test]
fn before_its_start_instant_the_playback_stays_silent_and_keeps_its_frames() {
    let (cell, playback) = timed_cell(0, 100 * MS);
    fill(&playback, 16);
    let mut out = vec![0.0f32; 8 * 2];
    // This buffer is heard from 50 ms to 58 ms — all before the start.
    mix_di_playback_at(&cell, &mut out, 2, Some(50 * MS));
    assert!(
        out.iter().all(|s| *s == 0.0),
        "nothing sounds before the start"
    );
    assert_eq!(playback.ring().len(), 32, "no frame was consumed");
    assert_eq!(playback.play_pos(), 0);
}

#[test]
fn the_first_frame_lands_on_the_exact_frame_of_the_start_instant() {
    let (cell, playback) = timed_cell(0, 103 * MS);
    fill(&playback, 16);
    let mut out = vec![0.0f32; 8 * 2];
    // Heard from 100 ms: frames 0..3 are before the start.
    mix_di_playback_at(&cell, &mut out, 2, Some(100 * MS));
    assert_eq!(lefts(&out), vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(playback.play_pos(), 5);
}

#[test]
fn a_late_playback_that_is_late_by_a_whole_loop_skips_only_the_phase() {
    // Late by 2005 frames on a 2000-frame loop: in phase, it is 5 frames in.
    let (cell, playback) = timed_cell(0, 100 * MS);
    fill(&playback, 16);
    let mut out = vec![0.0f32; 2 * 2];
    mix_di_playback_at(&cell, &mut out, 2, Some(2_105 * MS));
    assert_eq!(lefts(&out), vec![6.0, 7.0]);
}

#[test]
fn a_late_playback_skips_what_it_missed_and_plays_in_phase() {
    let (cell, playback) = timed_cell(40, 100 * MS);
    fill(&playback, 32);
    let mut out = vec![0.0f32; 8 * 2];
    // The first buffer is heard 5 ms after the start: frames 0..5 of the take
    // are already in the past, so the playback resumes at frame 5.
    mix_di_playback_at(&cell, &mut out, 2, Some(105 * MS));
    assert_eq!(
        lefts(&out),
        vec![6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0]
    );
    assert_eq!(
        playback.play_pos(),
        40 + 13,
        "the skipped frames count: the position is where the timeline is"
    );
}

#[test]
fn a_late_playback_keeps_skipping_until_it_has_caught_up() {
    let (cell, playback) = timed_cell(0, 100 * MS);
    fill(&playback, 4);
    let mut out = vec![0.0f32; 8 * 2];
    // 10 frames late with only 4 rendered: drop all 4, owe 6 — and the 8
    // frames of this buffer pass with nothing to play, so the debt is 14.
    mix_di_playback_at(&cell, &mut out, 2, Some(110 * MS));
    assert!(out.iter().all(|s| *s == 0.0));
    let ring = playback.ring();
    for i in 4..20 {
        assert!(ring.push(marker(i)));
        assert!(ring.push(0.0));
    }
    let mut out = vec![0.0f32; 4 * 2];
    mix_di_playback_at(&cell, &mut out, 2, Some(118 * MS));
    assert_eq!(
        lefts(&out),
        vec![19.0, 20.0, 0.0, 0.0],
        "the debt is paid first: at 118 ms the timeline is at frame 18"
    );
}

#[test]
fn a_timed_playback_that_runs_dry_stays_on_the_timeline() {
    let (cell, playback) = timed_cell(0, 100 * MS);
    fill(&playback, 2);
    let mut out = vec![0.0f32; 4 * 2];
    // On time, but only 2 of 4 frames are rendered: frames 2 and 3 are owed.
    mix_di_playback_at(&cell, &mut out, 2, Some(100 * MS));
    assert_eq!(lefts(&out), vec![1.0, 2.0, 0.0, 0.0]);
    let ring = playback.ring();
    for i in 2..8 {
        assert!(ring.push(marker(i)));
        assert!(ring.push(0.0));
    }
    let mut out = vec![0.0f32; 2 * 2];
    mix_di_playback_at(&cell, &mut out, 2, Some(104 * MS));
    assert_eq!(lefts(&out), vec![5.0, 6.0], "frame 4 plays at 104 ms");
}

#[test]
fn without_a_host_time_the_playback_starts_at_once() {
    let (cell, playback) = timed_cell(0, 100 * MS);
    fill(&playback, 4);
    let mut out = vec![0.0f32; 2 * 2];
    mix_di_playback_at(&cell, &mut out, 2, None);
    assert_eq!(lefts(&out), vec![1.0, 2.0]);
}

#[test]
fn an_untimed_playback_ignores_the_host_time() {
    let playback = Arc::new(DiPlayback::starting_at(0, 1, 2_000, 0));
    let cell: DiPlaybackCell = Arc::new(ArcSwapOption::from(Some(Arc::clone(&playback))));
    fill(&playback, 4);
    let mut out = vec![0.0f32; 2 * 2];
    mix_di_playback_at(&cell, &mut out, 2, Some(5 * MS));
    assert_eq!(lefts(&out), vec![1.0, 2.0]);
}
