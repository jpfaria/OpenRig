//! A loop must play back exactly as long as its take.
//!
//! Every loop sits on one shared timeline whose cycle is a whole number of
//! take frames. A playback that is even a few frames shorter than its take
//! reaches the end of each turn early and walks off the timeline, one turn at
//! a time: measured on the rig, two loops on two chains drifted ~3 ms per
//! second apart because the isolated stream trimmed a 10 ms seam crossfade
//! off every take.

use project::chain::LooperSpeed;

use super::controller_loopers::looper_playback_pcm;

const RATE: u32 = 44_100;
/// An odd length, like a real take closed on the timeline.
const FRAMES: usize = 89_295;

fn played_frames(speed: LooperSpeed) -> usize {
    looper_playback_pcm(vec![0.5_f32; FRAMES * 2], RATE, speed)
        .to_loop_at(RATE)
        .len()
}

#[test]
fn a_loop_plays_back_exactly_as_long_as_its_take() {
    assert_eq!(played_frames(LooperSpeed::Normal), FRAMES);
}

#[test]
fn a_half_speed_loop_plays_back_exactly_twice_its_take() {
    assert_eq!(played_frames(LooperSpeed::Half), FRAMES * 2);
}
