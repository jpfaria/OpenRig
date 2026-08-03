//! #826 — the pure loop-edit transforms. Interleaved stereo, frame indices,
//! no I/O, no engine state.

use super::*;

/// A ramp: frame `i` is `[i, -i]`, so both channels and the frame order are
/// visible in every assertion.
fn ramp(frames: usize) -> Vec<f32> {
    (0..frames).flat_map(|i| [i as f32, -(i as f32)]).collect()
}

fn frame(pcm: &[f32], i: usize) -> [f32; 2] {
    [pcm[i * 2], pcm[i * 2 + 1]]
}

#[test]
fn trim_keeps_only_the_selected_frames() {
    let pcm = ramp(1024);
    let out = apply_edit(&pcm, LoopEditOp::Keep, 64, 704).unwrap();

    assert_eq!(
        out.len() / 2,
        640 - SEAM_FRAMES,
        "the overlap is folded in, not kept"
    );
    // Past the seam the body is the source, frame for frame.
    assert_eq!(frame(&out, SEAM_FRAMES), frame(&pcm, 64 + SEAM_FRAMES));
}

#[test]
fn crop_is_the_same_transform_as_trim() {
    let pcm = ramp(1024);
    let trimmed = apply_edit(&pcm, LoopEditOp::Keep, 64, 704).unwrap();
    let cropped = apply_edit(&pcm, LoopEditOp::Keep, 64, 704).unwrap();
    assert_eq!(
        trimmed, cropped,
        "crop and trim differ in intent, not in audio"
    );
}

#[test]
fn cut_removes_the_region_and_joins_the_halves() {
    let pcm = ramp(400);
    let out = apply_edit(&pcm, LoopEditOp::Cut, 100, 200).unwrap();

    // Two seams are folded in: the join between the halves, and the loop's
    // own wrap.
    assert_eq!(
        out.len() / 2,
        300 - SEAM_FRAMES * 2,
        "the removed region is gone"
    );
    // Mark the cut region with a value nothing else can blend into: the join
    // mixes the frames AROUND the cut, never the cut itself.
    let mut marked = pcm.clone();
    for s in marked[100 * 2..200 * 2].iter_mut() {
        *s = 9_999.0;
    }
    let out_marked = apply_edit(&marked, LoopEditOp::Cut, 100, 200).unwrap();
    assert!(
        out_marked.iter().all(|s| s.abs() < 1_000.0),
        "the cut region is gone, not merely faded"
    );
    // Past the join's overlap the body is the source again, frame for frame:
    // the second half resumes at its own frame `end + SEAM_FRAMES`.
    assert_eq!(frame(&out, 100 + 10), frame(&pcm, 200 + SEAM_FRAMES + 10));
}

#[test]
fn the_seam_is_a_blend_not_a_step() {
    // The point of the seam: the join is blended, and nothing overshoots the
    // source peak (equal-gain, #614).
    let pcm = ramp(400);
    let out = apply_edit(&pcm, LoopEditOp::Cut, 100, 200).unwrap();
    let peak = pcm.iter().fold(0.0f32, |a, s| a.max(s.abs()));

    assert_ne!(frame(&out, 100), frame(&pcm, 100), "the join is blended");
    assert!(
        out.iter().all(|s| s.abs() <= peak + 1e-3),
        "no overshoot at the seam"
    );
}

#[test]
fn an_out_of_range_or_empty_region_is_an_error_not_a_panic() {
    let pcm = ramp(256);
    assert_eq!(
        apply_edit(&pcm, LoopEditOp::Keep, 0, 999),
        Err(LoopEditError::OutOfRange)
    );
    assert_eq!(
        apply_edit(&pcm, LoopEditOp::Keep, 100, 100),
        Err(LoopEditError::EmptyRegion)
    );
    assert_eq!(
        apply_edit(&pcm, LoopEditOp::Keep, 200, 100),
        Err(LoopEditError::EmptyRegion)
    );
}

#[test]
fn an_edit_that_would_leave_almost_nothing_is_refused() {
    let pcm = ramp(256);
    assert_eq!(
        apply_edit(&pcm, LoopEditOp::Keep, 0, 8),
        Err(LoopEditError::ResultTooShort)
    );
    assert_eq!(
        apply_edit(&pcm, LoopEditOp::Cut, 8, 250),
        Err(LoopEditError::ResultTooShort)
    );
}

#[test]
fn peaks_bucket_the_envelope_into_zero_to_one() {
    // Frame i has amplitude i/255, so the last bucket peaks near 1.0 and the
    // first near 0.
    let pcm: Vec<f32> = (0..256)
        .flat_map(|i| {
            let a = i as f32 / 255.0;
            [a, -a]
        })
        .collect();
    let p = peaks(&pcm, 4);

    assert_eq!(p.len(), 4);
    assert!(p[0] < p[1] && p[1] < p[2] && p[2] < p[3]);
    assert!((p[3] - 1.0).abs() < 1e-3, "the loudest bucket reads 1.0");
    assert!(p.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn peaks_never_divides_by_zero_on_a_short_or_empty_loop() {
    assert_eq!(peaks(&[], 8), vec![0.0; 8]);
    assert_eq!(peaks(&ramp(2), 0), Vec::<f32>::new());
    assert_eq!(
        peaks(&ramp(3), 8).len(),
        8,
        "fewer frames than buckets still fills them"
    );
}

#[test]
fn content_bounds_find_where_the_playing_actually_starts_and_ends() {
    // #826: the take the player keeps is rarely the take they recorded — there
    // is a count-in at the head and a late release at the tail. "Fit" is the
    // one-button answer: find the music, drop the rest.
    let mut pcm = vec![0.0f32; 2000 * 2];
    // Music from frame 400 to 1500, at a healthy level.
    for f in 400..1500 {
        pcm[f * 2] = 0.6;
        pcm[f * 2 + 1] = -0.6;
    }

    let (start, end) = content_bounds(&pcm).expect("there is material");
    // Just BEFORE the first note: the margin is deliberate air, so an attack
    // is never clipped — but never more than that margin.
    assert!(
        (400 - CONTENT_MARGIN_FRAMES..=400).contains(&start),
        "the head is trimmed to just before the first note, not into it ({start})"
    );
    assert!(
        (1500..=1500 + CONTENT_MARGIN_FRAMES).contains(&end),
        "the tail keeps the last note whole ({end})"
    );
}

#[test]
fn content_bounds_ignore_a_noise_floor_but_keep_quiet_playing() {
    // A silent stretch is never truly zero: an interface's noise floor sits
    // around -60 dBFS. A quiet note must survive it.
    let mut pcm = vec![0.0f32; 1200 * 2];
    for (i, s) in pcm.iter_mut().enumerate() {
        *s = if i % 2 == 0 { 0.0005 } else { -0.0005 };
    }
    for f in 600..900 {
        pcm[f * 2] = 0.05;
        pcm[f * 2 + 1] = -0.05;
    }

    let (start, end) = content_bounds(&pcm).expect("quiet playing is still playing");
    assert!(
        (500..=600).contains(&start),
        "the noise floor is not music ({start})"
    );
    assert!(
        (900..=1000).contains(&end),
        "the quiet note is kept ({end})"
    );
}

#[test]
fn content_bounds_report_nothing_on_a_silent_take() {
    assert_eq!(content_bounds(&vec![0.0f32; 1000 * 2]), None);
    assert_eq!(content_bounds(&[]), None);
}

#[test]
fn fitting_a_take_trims_the_silence_off_both_ends() {
    let mut pcm = vec![0.0f32; 4000 * 2];
    for f in 800..3000 {
        pcm[f * 2] = 0.5;
        pcm[f * 2 + 1] = -0.5;
    }

    let out = apply_edit(&pcm, LoopEditOp::Fit, 0, 0).expect("a take with music can be fitted");
    let fitted = out.len() / 2;
    assert!(
        (2100..=2300).contains(&fitted),
        "the fitted loop is about the music's own length ({fitted})"
    );
    let head_peak = out[..200].iter().fold(0.0f32, |a, s| a.max(s.abs()));
    assert!(head_peak > 0.1, "the fitted loop starts on the music");
}

#[test]
fn fitting_a_silent_take_is_refused_rather_than_leaving_a_click() {
    assert_eq!(
        apply_edit(&vec![0.0f32; 4000 * 2], LoopEditOp::Fit, 0, 0),
        Err(LoopEditError::EmptyRegion)
    );
}

#[test]
fn fit_trims_a_take_whose_silence_is_real_world_hiss() {
    // The user pressed FIT and nothing happened. A guitar take's "silence" is
    // never -60 dBFS: a single-coil into a live rig hisses around -45 dBFS, so
    // an absolute floor finds nothing to trim and the button looks dead. What
    // matters is the take's OWN dynamic range: hiss sits far below the playing.
    let mut pcm = vec![0.0f32; 4000 * 2];
    for (i, s) in pcm.iter_mut().enumerate() {
        // ~-45 dBFS hiss everywhere, alternating so it is not DC.
        *s = if i % 2 == 0 { 0.0056 } else { -0.0056 };
    }
    for f in 1000..3000 {
        pcm[f * 2] = 0.5;
        pcm[f * 2 + 1] = -0.5;
    }

    let (start, end) = content_bounds(&pcm).expect("the playing is well above the hiss");
    assert!(
        start > 900 && start <= 1000,
        "the hiss before the first note is not the take ({start})"
    );
    assert!(
        (3000..=3100).contains(&end),
        "the hiss after the last note is not the take ({end})"
    );
}
