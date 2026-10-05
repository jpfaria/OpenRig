//! #979 — the player's callback is built from the layout of the stream it
//! opened on, without a device: the track lands on the layout's target
//! channels and the render stays silent while the player is paused.

use std::sync::Arc;

use engine::player::settings::PlayerSettings;
use engine::player::shared::PlayerShared;
use engine::spsc::SpscRing;

use super::player_render;
use crate::aux_output::AuxOutputLayout;

fn layout() -> AuxOutputLayout {
    AuxOutputLayout {
        sample_rate: 44_100,
        channels: 4,
        targets: vec![2, 3],
        max_frames: 64,
    }
}

fn ring_with_signal() -> Arc<SpscRing<f32>> {
    let ring = Arc::new(SpscRing::new(4096, 0.0));
    for _ in 0..2048 {
        ring.push(0.5);
    }
    ring
}

#[test]
fn the_track_plays_on_the_layout_target_channels() {
    let shared = Arc::new(PlayerShared::new(PlayerSettings::default()));
    shared.set_playing(true);
    let mut render = player_render(shared, ring_with_signal())(&layout());
    let mut out = vec![0.0f32; 64 * 4];

    render(&mut out);

    let on = |channel: usize| out.chunks(4).any(|frame| frame[channel] != 0.0);
    assert!(on(2) && on(3), "the track must reach the target channels");
    assert!(
        !on(0) && !on(1),
        "the track must stay off the other channels"
    );
}

#[test]
fn the_render_is_silent_while_paused() {
    let shared = Arc::new(PlayerShared::new(PlayerSettings::default()));
    let mut render = player_render(shared, ring_with_signal())(&layout());
    let mut out = vec![1.0f32; 64 * 4];

    render(&mut out);

    assert!(out.iter().all(|sample| *sample == 0.0));
}
