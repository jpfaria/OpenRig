use super::*;

const CHANNELS: usize = 2;
const BUFFER: usize = 64;

/// One callback of a constant 1.0 through `fade`: what comes out is the gain.
fn callback(fade: &mut OutputFade) -> Vec<f32> {
    let mut out = vec![1.0f32; BUFFER * CHANNELS];
    fade.apply(&mut out, CHANNELS);
    out
}

/// The gain of every frame `fade` plays over `frames` frames.
fn gains(fade: &mut OutputFade, frames: usize) -> Vec<f32> {
    (0..frames / BUFFER)
        .flat_map(|_| callback(fade).into_iter().step_by(CHANNELS))
        .collect()
}

#[test]
fn streams_that_replace_nothing_play_bit_identical() {
    let set = StreamHandover::cold();
    let mut fade = set.output_fade();
    let mut out: Vec<f32> = (0..BUFFER * CHANNELS)
        .map(|i| i as f32 * 0.01 - 0.3)
        .collect();
    let expected = out.clone();
    fade.apply(&mut out, CHANNELS);
    assert_eq!(out, expected);
}

#[test]
fn new_streams_are_unheard_through_the_warm_up_then_fade_in_to_full_level() {
    let set = StreamHandover::replacing();
    let mut fade = set.output_fade();
    let heard = gains(
        &mut fade,
        STREAM_WARMUP_FRAMES + STREAM_FADE_FRAMES + 4 * BUFFER,
    );
    assert!(
        heard[..STREAM_WARMUP_FRAMES].iter().all(|&g| g == 0.0),
        "the new streams are silent through the warm-up"
    );
    let fade_in = &heard[STREAM_WARMUP_FRAMES..STREAM_WARMUP_FRAMES + STREAM_FADE_FRAMES];
    assert!(
        fade_in.windows(2).all(|w| w[1] >= w[0]),
        "the fade-in only rises"
    );
    assert!(
        fade_in
            .windows(2)
            .all(|w| w[1] - w[0] < 4.0 / STREAM_FADE_FRAMES as f32),
        "the fade-in has no step"
    );
    assert!(
        heard[STREAM_WARMUP_FRAMES + STREAM_FADE_FRAMES..]
            .iter()
            .all(|&g| g == 1.0),
        "then the new streams play at full level"
    );
}

#[test]
fn old_and_new_streams_sum_to_full_level_through_the_whole_handover() {
    let old = StreamHandover::cold();
    let mut old_fade = old.output_fade();
    for _ in 0..100 {
        callback(&mut old_fade);
    }
    let new = StreamHandover::replacing();
    old.retire_into(Arc::clone(&new));
    let mut new_fade = new.output_fade();
    for cycle in 0..(STREAM_WARMUP_FRAMES + STREAM_FADE_FRAMES) / BUFFER + 8 {
        // Both streams run in the same device cycle, the old one first.
        let old_out = callback(&mut old_fade);
        let new_out = callback(&mut new_fade);
        for (frame, (o, n)) in old_out.iter().zip(new_out.iter()).enumerate() {
            assert!(
                (o + n - 1.0).abs() < 1e-6,
                "cycle {cycle} sample {frame}: old {o} + new {n} is not full level"
            );
        }
    }
    assert!(new.has_taken_over());
    assert!(
        callback(&mut old_fade).iter().all(|&g| g == 0.0),
        "after the takeover the old streams are silent"
    );
}

#[test]
fn old_streams_keep_full_level_while_the_new_ones_have_not_played() {
    let old = StreamHandover::cold();
    let mut old_fade = old.output_fade();
    old.retire_into(StreamHandover::replacing());
    for _ in 0..200 {
        assert!(
            callback(&mut old_fade).iter().all(|&g| g == 1.0),
            "no gap: the old streams play on until the new ones are heard"
        );
    }
}

#[test]
fn a_new_set_takes_over_only_after_its_warm_up_and_fade() {
    let new = StreamHandover::replacing();
    let mut fade = new.output_fade();
    assert!(!new.has_taken_over());
    gains(&mut fade, STREAM_WARMUP_FRAMES);
    assert!(!new.has_taken_over(), "still warming up");
    gains(&mut fade, STREAM_FADE_FRAMES);
    assert!(new.has_taken_over());
    assert!(
        StreamHandover::cold().has_taken_over(),
        "a set that replaces nothing plays on its own from the start"
    );
}
