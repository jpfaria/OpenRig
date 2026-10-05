use super::*;

fn sine(rate: u32, hz: f32, seconds: f32) -> PlayerPcm {
    let frames = (rate as f32 * seconds) as usize;
    let samples = (0..frames)
        .flat_map(|i| {
            let s = (i as f32 / rate as f32 * hz * std::f32::consts::TAU).sin() * 0.5;
            [s, s]
        })
        .collect();
    PlayerPcm::from_stereo(samples, rate)
}

fn zero_crossings(pcm: &PlayerPcm) -> usize {
    let left: Vec<f32> = (0..pcm.frames()).map(|i| pcm.frame(i)[0]).collect();
    left.windows(2)
        .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
        .count()
}

#[test]
fn a_track_at_the_device_rate_is_untouched() {
    let pcm = sine(48_000, 440.0, 0.1);
    let same = resample_to(pcm.clone(), 48_000).unwrap();
    assert_eq!(same, pcm);
}

#[test]
fn resampling_keeps_duration_and_pitch() {
    let pcm = sine(44_100, 440.0, 1.0);
    let out = resample_to(pcm, 48_000).unwrap();
    assert_eq!(out.sample_rate(), 48_000);
    let frames = out.frames() as i64;
    assert!((frames - 48_000).abs() <= 2, "frames = {frames}");
    let cycles = zero_crossings(&out) as i64;
    assert!((cycles - 440).abs() <= 2, "cycles = {cycles}");
}

#[test]
fn a_zero_output_rate_is_rejected() {
    assert!(resample_to(sine(48_000, 440.0, 0.01), 0).is_err());
}
