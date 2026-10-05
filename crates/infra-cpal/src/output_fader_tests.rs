use super::*;
use domain::mixer_strip::MixerDirection;

fn set_gain(device: &str, channels: &[usize], linear: f32) {
    engine::mixer_gains::set_endpoint_gain(MixerDirection::Output, device, channels, linear);
}

#[test]
fn fader_at_minimum_silences_only_the_pipeline_channels() {
    set_gain("fader-test-min", &[2, 3], 0.0);
    let fader = OutputFader::of("fader-test-min", &[2, 3]);
    let mut out = vec![1.0f32; 4 * 8];
    fader.apply(&mut out, 4, &[2, 3]);
    for frame in out.chunks(4) {
        assert_eq!(frame, &[1.0, 1.0, 0.0, 0.0]);
    }
}

#[test]
fn untouched_fader_leaves_the_signal_bit_identical() {
    let fader = OutputFader::of("fader-test-unity", &[0, 1]);
    let mut out: Vec<f32> = (0..16).map(|i| i as f32 * 0.03125).collect();
    let expected = out.clone();
    fader.apply(&mut out, 2, &[0, 1]);
    assert_eq!(out, expected);
}

#[test]
fn fader_move_glides_to_the_new_level_within_one_callback() {
    let fader = OutputFader::of("fader-test-glide", &[0, 1]);
    set_gain("fader-test-glide", &[0, 1], 0.5);
    let mut out = vec![1.0f32; 2 * 4];
    fader.apply(&mut out, 2, &[0, 1]);
    assert!(
        out[0] < 1.0 && out[0] > 0.5,
        "first frame mid-glide, got {}",
        out[0]
    );
    assert_eq!(&out[6..8], &[0.5, 0.5], "last frame lands on the target");
    let mut next = vec![1.0f32; 2 * 4];
    fader.apply(&mut next, 2, &[0, 1]);
    assert!(next.iter().all(|s| *s == 0.5), "then holds the target");
}

#[test]
fn faded_render_obeys_the_endpoint_fader() {
    set_gain("fader-test-render", &[24, 25], 0.0);
    let layout = AuxOutputLayout {
        sample_rate: 48_000,
        channels: 26,
        targets: vec![24, 25],
        max_frames: 64,
    };
    let render: AuxRender = Box::new(|out: &mut [f32]| out.fill(1.0));
    let mut faded = faded_render("fader-test-render", &[24, 25], &layout, render);
    let mut out = vec![0.0f32; 26 * 4];
    faded(&mut out);
    for frame in out.chunks(26) {
        assert_eq!(frame[24], 0.0);
        assert_eq!(frame[25], 0.0);
        assert_eq!(
            frame[0], 1.0,
            "channels outside the endpoint keep the render"
        );
    }
}
