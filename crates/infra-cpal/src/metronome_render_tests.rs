//! #979 — the metronome's callback is built from the layout of the stream it
//! opened on, without a device: the click lands on the layout's target
//! channels and the render stays silent while the click is off.

use std::sync::Arc;

use engine::metronome_state::{MetronomeSettings, MetronomeShared};

use super::metronome_render;
use crate::aux_output::AuxOutputLayout;

fn layout() -> AuxOutputLayout {
    AuxOutputLayout {
        sample_rate: 44_100,
        channels: 4,
        targets: vec![2, 3],
        max_frames: 64,
    }
}

#[test]
fn the_click_plays_on_the_layout_target_channels() {
    let shared = Arc::new(MetronomeShared::new(MetronomeSettings::default()));
    shared.set_enabled(true);
    let mut render = metronome_render(Arc::clone(&shared))(&layout());
    let mut out = vec![0.0f32; 64 * 4];

    render(&mut out);

    let on = |channel: usize| out.chunks(4).any(|frame| frame[channel] != 0.0);
    assert!(on(2) && on(3), "the click must reach the target channels");
    assert!(
        !on(0) && !on(1),
        "the click must stay off the other channels"
    );
}

#[test]
fn the_render_is_silent_while_the_click_is_off() {
    let shared = Arc::new(MetronomeShared::new(MetronomeSettings::default()));
    let mut render = metronome_render(shared)(&layout());
    let mut out = vec![1.0f32; 64 * 4];

    render(&mut out);

    assert!(out.iter().all(|sample| *sample == 0.0));
}
