use super::*;

const RATE: u32 = 48_000;

/// A track whose left sample is its own frame index, so every read can be
/// traced back to the source position it came from.
fn ramp_track(frames: usize) -> Arc<PlayerPcm> {
    let samples = (0..frames).flat_map(|i| [i as f32, -(i as f32)]).collect();
    Arc::new(PlayerPcm::from_stereo(samples, RATE))
}

fn sine_track(hz: f32, seconds: f32) -> Arc<PlayerPcm> {
    let frames = (RATE as f32 * seconds) as usize;
    let samples = (0..frames)
        .flat_map(|i| {
            let s = (i as f32 / RATE as f32 * hz * std::f32::consts::TAU).sin() * 0.5;
            [s, s]
        })
        .collect();
    Arc::new(PlayerPcm::from_stereo(samples, RATE))
}

fn settings(speed: f32, semitones: f32) -> PlayerSettings {
    PlayerSettings {
        speed,
        semitones,
        ..PlayerSettings::default()
    }
}

fn render_all(renderer: &mut PlayerRenderer, block: usize, limit: usize) -> Vec<f32> {
    let mut all = Vec::new();
    let mut out = vec![0.0; block * 2];
    while all.len() / 2 < limit {
        let written = renderer.render(&mut out);
        all.extend_from_slice(&out[..written * 2]);
        if written < block {
            break;
        }
    }
    all
}

fn rising_zero_crossings(left: impl Iterator<Item = f32>) -> usize {
    let mut previous = 0.0;
    let mut count = 0;
    for sample in left {
        if previous <= 0.0 && sample > 0.0 {
            count += 1;
        }
        previous = sample;
    }
    count
}

#[test]
fn unity_playback_copies_the_track_untouched() {
    let mut renderer = PlayerRenderer::new(ramp_track(1000), PlayerSettings::default());
    let mut out = vec![0.0; 8];
    assert_eq!(renderer.render(&mut out), 4);
    assert_eq!(out, vec![0.0, -0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0]);
}

#[test]
fn unity_playback_ends_with_a_short_block() {
    let mut renderer = PlayerRenderer::new(ramp_track(600), PlayerSettings::default());
    let all = render_all(&mut renderer, 256, usize::MAX);
    assert_eq!(all.len() / 2, 600);
    assert!(renderer.ended());
    let mut out = vec![0.0; 8];
    assert_eq!(renderer.render(&mut out), 0);
}

#[test]
fn seek_moves_the_read_position() {
    let mut renderer = PlayerRenderer::new(ramp_track(RATE as usize), PlayerSettings::default());
    renderer.seek_seconds(0.5);
    let mut out = vec![0.0; 2];
    renderer.render(&mut out);
    assert_eq!(out[0], 24_000.0);
    assert_eq!(renderer.position_seconds(), 24_001.0 / RATE as f64);
}

#[test]
fn seeking_after_the_end_plays_again() {
    let mut renderer = PlayerRenderer::new(ramp_track(100), PlayerSettings::default());
    render_all(&mut renderer, 64, usize::MAX);
    assert!(renderer.ended());
    renderer.seek_seconds(0.0);
    assert!(!renderer.ended());
    let mut out = vec![0.0; 2];
    assert_eq!(renderer.render(&mut out), 1);
}

#[test]
fn a_loop_never_ends_and_returns_to_its_start() {
    let loop_settings = PlayerSettings {
        loop_range: Some((0.5, 1.0)),
        ..PlayerSettings::default()
    };
    let mut renderer = PlayerRenderer::new(ramp_track(2 * RATE as usize), loop_settings);
    renderer.seek_seconds(0.9);
    let all = render_all(&mut renderer, 256, RATE as usize);
    assert!(!renderer.ended());
    assert_eq!(all.len() / 2, RATE as usize);
    let crossfade = (RATE as f64 * LOOP_CROSSFADE_SECONDS) as usize;
    let left: Vec<f32> = all.iter().step_by(2).copied().collect();
    let first_after_seam = left
        .iter()
        .position(|&s| s == (24_000 + crossfade) as f32)
        .expect("playback resumes right after the crossfade into the loop start");
    assert_eq!(first_after_seam, 4_800 + crossfade);
    assert_eq!(left[first_after_seam + 1], (24_001 + crossfade) as f32);
}

#[test]
fn seeking_outside_a_loop_lands_on_its_start() {
    let loop_settings = PlayerSettings {
        loop_range: Some((0.5, 1.0)),
        ..PlayerSettings::default()
    };
    let mut renderer = PlayerRenderer::new(ramp_track(2 * RATE as usize), loop_settings);
    renderer.seek_seconds(1.5);
    let mut out = vec![0.0; 2];
    renderer.render(&mut out);
    assert_eq!(out[0], 24_000.0);
}

#[test]
fn volume_only_changes_need_no_restart() {
    let mut renderer = PlayerRenderer::new(ramp_track(1000), PlayerSettings::default());
    assert!(!renderer.apply(PlayerSettings {
        volume: 0.2,
        ..PlayerSettings::default()
    }));
}

#[test]
fn switching_the_stretcher_in_or_out_needs_a_restart() {
    let mut renderer = PlayerRenderer::new(ramp_track(1000), PlayerSettings::default());
    assert!(renderer.apply(settings(0.75, 0.0)));
    assert!(
        !renderer.apply(settings(0.5, 0.0)),
        "speed change while stretching"
    );
    assert!(renderer.apply(settings(1.0, 0.0)));
}

#[test]
fn a_loop_that_excludes_the_play_position_needs_a_restart() {
    let mut renderer =
        PlayerRenderer::new(ramp_track(2 * RATE as usize), PlayerSettings::default());
    renderer.seek_seconds(1.5);
    assert!(renderer.apply(PlayerSettings {
        loop_range: Some((0.2, 0.8)),
        ..PlayerSettings::default()
    }));
    let mut keeps = PlayerRenderer::new(ramp_track(2 * RATE as usize), PlayerSettings::default());
    keeps.seek_seconds(0.5);
    assert!(!keeps.apply(PlayerSettings {
        loop_range: Some((0.2, 0.8)),
        ..PlayerSettings::default()
    }));
}

#[test]
fn half_speed_takes_twice_as_long() {
    let mut renderer = PlayerRenderer::new(sine_track(440.0, 1.0), settings(0.5, 0.0));
    let all = render_all(&mut renderer, 256, 10 * RATE as usize);
    assert!(renderer.ended());
    let seconds = all.len() as f64 / 2.0 / RATE as f64;
    assert!((seconds - 2.0).abs() < 0.15, "rendered {seconds} s");
}

#[test]
fn double_speed_takes_half_as_long() {
    let mut renderer = PlayerRenderer::new(sine_track(440.0, 1.0), settings(2.0, 0.0));
    let all = render_all(&mut renderer, 256, 10 * RATE as usize);
    let seconds = all.len() as f64 / 2.0 / RATE as f64;
    assert!((seconds - 0.5).abs() < 0.15, "rendered {seconds} s");
}

#[test]
fn slowing_down_keeps_the_pitch() {
    let mut renderer = PlayerRenderer::new(sine_track(440.0, 2.0), settings(0.5, 0.0));
    renderer.seek_seconds(0.5);
    let all = render_all(&mut renderer, 256, RATE as usize);
    let cycles = rising_zero_crossings(all.iter().step_by(2).copied());
    assert!(
        (cycles as i64 - 440).abs() <= 10,
        "cycles in 1 s = {cycles}"
    );
}

#[test]
fn an_octave_up_doubles_the_pitch_at_the_same_speed() {
    let mut renderer = PlayerRenderer::new(sine_track(220.0, 3.0), settings(1.0, 12.0));
    renderer.seek_seconds(0.5);
    let all = render_all(&mut renderer, 256, RATE as usize);
    assert_eq!(
        all.len() / 2,
        RATE as usize,
        "same speed renders a full second"
    );
    let cycles = rising_zero_crossings(all.iter().step_by(2).copied());
    assert!(
        (cycles as i64 - 440).abs() <= 10,
        "cycles in 1 s = {cycles}"
    );
}

#[test]
fn stretched_position_tracks_the_source() {
    let mut renderer = PlayerRenderer::new(sine_track(440.0, 4.0), settings(0.5, 0.0));
    renderer.seek_seconds(1.0);
    render_all(&mut renderer, 256, RATE as usize);
    let position = renderer.position_seconds();
    assert!((position - 1.5).abs() < 0.1, "position {position}");
}
