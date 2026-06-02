use super::*;
use block_core::AudioChannelLayout;

#[test]
fn from_samples_mono_no_resample_preserves_len_and_layout() {
    let samples = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, 0);
    assert_eq!(di.layout(), AudioChannelLayout::Mono);
    assert_eq!(di.len(), 8);
}

#[test]
fn from_samples_stereo_deinterleaves_to_stereo_frames() {
    let samples = vec![0.1, 0.2, 0.3, 0.4];
    let di = DiLoop::from_samples(&samples, 48_000, 2, 48_000, 0);
    assert_eq!(di.layout(), AudioChannelLayout::Stereo);
    assert_eq!(di.len(), 2);
    match di.frame_at(0) {
        DiFrame::Stereo([l, r]) => assert!((l - 0.1).abs() < 1e-6 && (r - 0.2).abs() < 1e-6),
        _ => panic!("expected stereo"),
    }
}

#[test]
fn frame_at_wraps_around() {
    let samples = vec![0.0, 1.0, 2.0];
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, 0);
    match (di.frame_at(0), di.frame_at(3), di.frame_at(4)) {
        (DiFrame::Mono(a), DiFrame::Mono(b), DiFrame::Mono(c)) => {
            assert_eq!(a, 0.0);
            assert_eq!(b, 0.0);
            assert_eq!(c, 1.0);
        }
        _ => panic!("mono expected"),
    }
}

#[test]
fn resample_doubles_length_when_target_is_double() {
    let samples = vec![0.0, 0.25, 0.5, 0.75];
    let di = DiLoop::from_samples(&samples, 24_000, 1, 48_000, 0);
    assert!((di.len() as i64 - 8).abs() <= 1, "len was {}", di.len());
}

#[test]
fn loop_crossfade_makes_seam_continuous() {
    let n = 256;
    let samples: Vec<f32> = (0..n).map(|i| i as f32 / n as f32).collect();
    let xfade = 32;
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, xfade);
    let last = match di.frame_at(di.len() - 1) { DiFrame::Mono(s) => s, _ => unreachable!() };
    let first = match di.frame_at(0) { DiFrame::Mono(s) => s, _ => unreachable!() };
    let seam_step = (first - last).abs();
    assert!(seam_step < 0.5, "seam step {seam_step} not reduced by crossfade");
}
