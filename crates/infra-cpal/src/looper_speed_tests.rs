//! #903 — the looper's `speed` must reach playback.
//!
//! A loop plays on its isolated stream, which sources the store's exported
//! mixdown. Nothing between the store and that stream ever looked at
//! `LooperConfig.speed`, so half and double were settings that changed the
//! project file and nothing else — the owner's "speed não funciona".
//!
//! Speed is not a resample: the read cursor steps by the factor, so the pitch
//! follows it (classic looper behaviour). Declaring the source rate scaled by
//! the factor gives exactly that when the stream resamples to its output rate.

use project::chain::LooperSpeed;

use super::controller_loopers::looper_playback_pcm;

const RATE: u32 = 48_000;
const FRAMES: usize = 48_000;

fn take() -> Vec<f32> {
    vec![0.5_f32; FRAMES * 2]
}

/// Frames the isolated stream reads for one turn of the loop, at its own rate.
/// `to_loop_at` trims a crossfade off the end, so the meaningful comparison is
/// against the same take at normal speed, not the raw recorded length.
fn played_frames(speed: LooperSpeed) -> usize {
    looper_playback_pcm(take(), RATE, speed)
        .to_loop_at(RATE)
        .len()
}

#[test]
fn half_speed_takes_about_twice_as_long_to_play_the_take() {
    let ratio = played_frames(LooperSpeed::Half) as f64 / played_frames(LooperSpeed::Normal) as f64;

    assert!(
        (ratio - 2.0).abs() < 0.05,
        "half speed must take about twice as long as normal — ratio was {ratio:.3}"
    );
}

#[test]
fn double_speed_takes_about_half_as_long_to_play_the_take() {
    let ratio =
        played_frames(LooperSpeed::Double) as f64 / played_frames(LooperSpeed::Normal) as f64;

    assert!(
        (ratio - 0.5).abs() < 0.05,
        "double speed must take about half as long as normal — ratio was {ratio:.3}"
    );
}
