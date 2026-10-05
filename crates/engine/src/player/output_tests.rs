use super::*;
use crate::player::settings::PlayerSettings;

fn shared_at(volume: f32) -> PlayerShared {
    PlayerShared::new(PlayerSettings {
        volume,
        ..PlayerSettings::default()
    })
}

fn ring_with(frames: usize, left: f32, right: f32) -> SpscRing<f32> {
    let ring = SpscRing::new(PLAYER_RING_SAMPLES, 0.0);
    for _ in 0..frames {
        ring.push(left);
        ring.push(right);
    }
    ring
}

#[test]
fn paused_player_is_silent_and_keeps_the_ring() {
    let shared = shared_at(1.0);
    let ring = ring_with(64, 0.5, 0.5);
    let mut state = PlayerOutputState::default();
    let mut out = vec![1.0; 128];
    fill_player_buffer(&mut state, &shared, &ring, &mut out, 2, &[]);
    assert!(out.iter().all(|&s| s == 0.0));
    assert_eq!(ring.len(), 128);
    assert_eq!(shared.consumed(), 0);
}

#[test]
fn playing_ramps_up_then_holds_the_volume() {
    let shared = shared_at(0.5);
    shared.set_playing(true);
    let ring = ring_with(1024, 1.0, -1.0);
    let mut state = PlayerOutputState::default();
    let mut out = vec![0.0; 1024 * 2];
    fill_player_buffer(&mut state, &shared, &ring, &mut out, 2, &[0, 1]);
    assert!(out[0] > 0.0 && out[0] < 0.01, "first frame starts the ramp");
    let last = &out[out.len() - 2..];
    assert_eq!(last, &[0.5, -0.5]);
    assert_eq!(shared.consumed(), 1024);
}

#[test]
fn one_target_gets_the_mono_sum() {
    let shared = shared_at(1.0);
    shared.set_playing(true);
    let ring = ring_with(512, 1.0, 0.0);
    let mut state = PlayerOutputState::default();
    let mut out = vec![0.0; 512 * 4];
    fill_player_buffer(&mut state, &shared, &ring, &mut out, 4, &[2]);
    let last = &out[out.len() - 4..];
    assert_eq!(last, &[0.0, 0.0, 0.5, 0.0]);
}

#[test]
fn two_targets_take_left_and_right() {
    let shared = shared_at(1.0);
    shared.set_playing(true);
    let ring = ring_with(512, 0.25, 0.75);
    let mut state = PlayerOutputState::default();
    let mut out = vec![0.0; 512 * 4];
    fill_player_buffer(&mut state, &shared, &ring, &mut out, 4, &[3, 2]);
    let last = &out[out.len() - 4..];
    assert_eq!(last, &[0.0, 0.0, 0.75, 0.25]);
}

#[test]
fn targets_out_of_range_fall_back_to_the_first_pair() {
    let shared = shared_at(1.0);
    shared.set_playing(true);
    let ring = ring_with(512, 0.25, 0.75);
    let mut state = PlayerOutputState::default();
    let mut out = vec![0.0; 512 * 2];
    fill_player_buffer(&mut state, &shared, &ring, &mut out, 2, &[9, 11]);
    assert_eq!(&out[out.len() - 2..], &[0.25, 0.75]);
}

#[test]
fn an_underrun_drops_to_silence_and_ramps_back() {
    let shared = shared_at(1.0);
    shared.set_playing(true);
    let ring = ring_with(300, 1.0, 1.0);
    let mut state = PlayerOutputState::default();
    let mut out = vec![0.0; 512 * 2];
    fill_player_buffer(&mut state, &shared, &ring, &mut out, 2, &[]);
    assert_eq!(state.gain(), 0.0);
    assert!(out[300 * 2..].iter().all(|&s| s == 0.0));
    assert_eq!(shared.consumed(), 300);
}

#[test]
fn a_flush_fades_out_then_empties_the_ring_and_acks() {
    let shared = shared_at(1.0);
    shared.set_playing(true);
    let ring = ring_with(2048, 1.0, 1.0);
    let mut state = PlayerOutputState::default();
    let mut out = vec![0.0; 512 * 2];
    fill_player_buffer(&mut state, &shared, &ring, &mut out, 2, &[]);
    assert_eq!(state.gain(), 1.0);

    let epoch = shared.request_flush();
    fill_player_buffer(&mut state, &shared, &ring, &mut out, 2, &[]);
    assert!(out[0] > 0.99, "the fade starts from the current level");
    assert_eq!(state.gain(), 0.0);
    assert!(ring.is_empty());
    assert!(shared.flush_done(epoch));
    assert_eq!(shared.consumed(), 2048, "drained frames count as consumed");
}

#[test]
fn a_flush_while_paused_acks_at_once() {
    let shared = shared_at(1.0);
    let ring = ring_with(64, 1.0, 1.0);
    let mut state = PlayerOutputState::default();
    let epoch = shared.request_flush();
    let mut out = vec![0.0; 64];
    fill_player_buffer(&mut state, &shared, &ring, &mut out, 2, &[]);
    assert!(shared.flush_done(epoch));
    assert!(ring.is_empty());
}
