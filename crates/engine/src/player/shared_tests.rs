use super::*;

#[test]
fn settings_round_trip_clamped_and_bump_the_generation() {
    let shared = PlayerShared::default();
    let before = shared.generation();
    shared.set_settings(PlayerSettings {
        volume: 0.5,
        speed: 5.0,
        semitones: 3.0,
        loop_range: Some((1.0, 4.0)),
    });
    assert!(shared.generation() > before);
    let read = shared.settings();
    assert_eq!(read.volume, 0.5);
    assert_eq!(read.speed, 2.0);
    assert_eq!(read.semitones, 3.0);
    assert_eq!(read.loop_range, Some((1.0, 4.0)));
}

#[test]
fn clearing_the_loop_reads_back_as_none() {
    let shared = PlayerShared::default();
    shared.set_settings(PlayerSettings {
        loop_range: Some((1.0, 4.0)),
        ..PlayerSettings::default()
    });
    shared.set_settings(PlayerSettings::default());
    assert_eq!(shared.settings().loop_range, None);
}

#[test]
fn each_seek_gets_a_new_epoch() {
    let shared = PlayerShared::default();
    let (first, _) = shared.seek_request();
    shared.request_seek(12.5);
    let (second, seconds) = shared.seek_request();
    assert_ne!(first, second);
    assert_eq!(seconds, 12.5);
}

#[test]
fn a_flush_is_pending_until_the_callback_acks_it() {
    let shared = PlayerShared::default();
    assert_eq!(shared.flush_pending(), None);
    let epoch = shared.request_flush();
    assert_eq!(shared.flush_pending(), Some(epoch));
    assert!(!shared.flush_done(epoch));
    shared.ack_flush(epoch);
    assert_eq!(shared.flush_pending(), None);
    assert!(shared.flush_done(epoch));
}
