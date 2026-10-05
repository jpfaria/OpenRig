use super::*;

#[test]
fn clamped_forces_every_field_into_range() {
    let wild = PlayerSettings {
        volume: 3.0,
        speed: 9.0,
        semitones: -40.0,
        loop_range: Some((-1.0, 10.0)),
    }
    .clamped();
    assert_eq!(wild.volume, 1.0);
    assert_eq!(wild.speed, PLAYER_SPEED_MAX);
    assert_eq!(wild.semitones, -PLAYER_SEMITONES_MAX);
    assert_eq!(wild.loop_range, Some((0.0, 10.0)));
}

#[test]
fn clamped_drops_a_reversed_or_tiny_loop() {
    let reversed = PlayerSettings {
        loop_range: Some((5.0, 2.0)),
        ..PlayerSettings::default()
    };
    assert_eq!(reversed.clamped().loop_range, None);
    let tiny = PlayerSettings {
        loop_range: Some((1.0, 1.1)),
        ..PlayerSettings::default()
    };
    assert_eq!(tiny.clamped().loop_range, None);
}

#[test]
fn clamped_replaces_non_finite_values() {
    let nan = PlayerSettings {
        volume: f32::NAN,
        speed: f32::INFINITY,
        semitones: f32::NAN,
        loop_range: Some((f64::NAN, 3.0)),
    }
    .clamped();
    assert_eq!(nan.volume, 0.0);
    assert_eq!(nan.speed, 1.0);
    assert_eq!(nan.semitones, 0.0);
    assert_eq!(nan.loop_range, None);
}

#[test]
fn unity_means_no_stretch_and_no_transpose() {
    assert!(PlayerSettings::default().is_unity());
    let slow = PlayerSettings {
        speed: 0.75,
        ..PlayerSettings::default()
    };
    assert!(!slow.is_unity());
    let up = PlayerSettings {
        semitones: 2.0,
        ..PlayerSettings::default()
    };
    assert!(!up.is_unity());
}
