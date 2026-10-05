use super::*;

fn decoded(samples: Vec<f32>, channels: usize) -> DecodedAudio {
    DecodedAudio {
        samples,
        channels,
        sample_rate: 48_000,
    }
}

#[test]
fn mono_is_broadcast_to_both_sides() {
    let pcm = PlayerPcm::from_decoded(decoded(vec![0.1, 0.2], 1)).unwrap();
    assert_eq!(pcm.frames(), 2);
    assert_eq!(pcm.frame(0), [0.1, 0.1]);
    assert_eq!(pcm.frame(1), [0.2, 0.2]);
}

#[test]
fn stereo_passes_through_untouched() {
    let pcm = PlayerPcm::from_decoded(decoded(vec![0.1, -0.1, 0.3, -0.3], 2)).unwrap();
    assert_eq!(pcm.frame(1), [0.3, -0.3]);
}

#[test]
fn surround_keeps_the_first_two_channels() {
    let pcm = PlayerPcm::from_decoded(decoded(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3)).unwrap();
    assert_eq!(pcm.frames(), 2);
    assert_eq!(pcm.frame(0), [1.0, 2.0]);
    assert_eq!(pcm.frame(1), [4.0, 5.0]);
}

#[test]
fn a_track_without_channels_or_rate_is_rejected() {
    assert!(PlayerPcm::from_decoded(decoded(vec![0.0], 0)).is_err());
    let no_rate = DecodedAudio {
        samples: vec![0.0, 0.0],
        channels: 2,
        sample_rate: 0,
    };
    assert!(PlayerPcm::from_decoded(no_rate).is_err());
}

#[test]
fn reading_past_the_end_is_silence() {
    let pcm = PlayerPcm::from_stereo(vec![0.5, 0.5], 48_000);
    assert_eq!(pcm.frame(7), [0.0, 0.0]);
}

#[test]
fn duration_and_frame_lookup_agree() {
    let pcm = PlayerPcm::from_stereo(vec![0.0; 96_000], 48_000);
    assert_eq!(pcm.duration_seconds(), 1.0);
    assert_eq!(pcm.frame_at(0.5), 24_000);
    assert_eq!(pcm.frame_at(-3.0), 0);
    assert_eq!(pcm.frame_at(99.0), 48_000);
    assert_eq!(pcm.frame_at(f64::NAN), 0);
}
