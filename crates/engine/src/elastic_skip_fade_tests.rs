use super::*;

#[test]
fn an_empty_fade_passes_frames_through_untouched() {
    let fade = SkipFade::none();
    let frame = AudioFrame::Stereo([0.3, -0.2]);
    let AudioFrame::Stereo(out) = fade.blend(0, frame) else {
        panic!("layout must be kept");
    };
    assert_eq!(out, [0.3, -0.2]);
}

#[test]
fn the_fade_moves_from_the_discarded_audio_to_the_kept_audio() {
    let mut fade = SkipFade::none();
    for _ in 0..FADE_FRAMES {
        fade.hold(AudioFrame::Mono(1.0));
    }
    let gains: Vec<f32> = (0..FADE_FRAMES)
        .map(|i| match fade.blend(i, AudioFrame::Mono(0.0)) {
            AudioFrame::Mono(s) => s,
            AudioFrame::Stereo(_) => panic!("layout must be kept"),
        })
        .collect();
    assert!(
        gains.windows(2).all(|w| w[1] < w[0]),
        "old audio must fade out: {gains:?}"
    );
    assert!(gains[0] < 1.0 && gains[FADE_FRAMES - 1] > 0.0);
    match fade.blend(FADE_FRAMES, AudioFrame::Mono(0.25)) {
        AudioFrame::Mono(s) => assert_eq!(s, 0.25, "past the fade the kept audio is untouched"),
        AudioFrame::Stereo(_) => panic!("layout must be kept"),
    }
}
