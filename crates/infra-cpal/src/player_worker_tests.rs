use super::*;
use engine::player::output::{fill_player_buffer, PlayerOutputState, PLAYER_RING_SAMPLES};
use engine::player::settings::PlayerSettings;
use engine::player::shared::PlayerShared;

const RATE: u32 = 48_000;

/// Decodes by file name: `ramp` is one second whose left sample is its frame
/// index, `short` is 1000 frames of the same, `cd` is that second at 44.1 kHz,
/// anything else fails.
fn fake_decode(path: &Path) -> Result<DecodedAudio, String> {
    let (frames, rate) = match path.to_str() {
        Some("ramp") => (RATE as usize, RATE),
        Some("short") => (1000, RATE),
        Some("cd") => (44_100, 44_100),
        _ => return Err("unreadable".into()),
    };
    Ok(DecodedAudio {
        samples: (0..frames).flat_map(|i| [i as f32, i as f32]).collect(),
        channels: 2,
        sample_rate: rate,
    })
}

struct Rig {
    shared: PlayerCell,
    ring: Arc<SpscRing<f32>>,
    worker: PlayerWorker,
    state: PlayerOutputState,
}

impl Rig {
    fn new(track: &str) -> Self {
        let shared: PlayerCell = Arc::new(PlayerShared::new(PlayerSettings {
            volume: 1.0,
            ..PlayerSettings::default()
        }));
        let ring = Arc::new(SpscRing::new(PLAYER_RING_SAMPLES, 0.0));
        let mut worker = PlayerWorker::new(Arc::clone(&shared), fake_decode);
        worker.handle(PlayerRequest::Load(PathBuf::from(track)));
        worker.handle(PlayerRequest::Attach {
            ring: Arc::clone(&ring),
            sample_rate: RATE,
        });
        Self {
            shared,
            ring,
            worker,
            state: PlayerOutputState::default(),
        }
    }

    /// One callback of `frames` frames on a stereo device, after a worker step.
    fn tick(&mut self, frames: usize) -> Vec<f32> {
        self.worker.step();
        let mut out = vec![0.0; frames * 2];
        fill_player_buffer(&mut self.state, &self.shared, &self.ring, &mut out, 2, &[]);
        self.worker.step();
        out
    }
}

#[test]
fn a_loaded_track_publishes_its_duration() {
    let rig = Rig::new("ramp");
    assert_eq!(rig.shared.duration_seconds(), 1.0);
    assert!(!rig.shared.is_loading());
    assert!(!rig.shared.has_failed());
}

#[test]
fn an_unreadable_track_is_flagged_failed() {
    let rig = Rig::new("missing");
    assert!(rig.shared.has_failed());
    assert_eq!(rig.shared.duration_seconds(), 0.0);
}

#[test]
fn paused_player_queues_audio_but_plays_none() {
    let mut rig = Rig::new("ramp");
    let out = rig.tick(256);
    assert!(out.iter().all(|&s| s == 0.0));
    assert!(
        rig.ring.len() >= 2 * 2048,
        "the queue is primed while paused"
    );
}

#[test]
fn playing_sends_the_track_from_its_start() {
    let mut rig = Rig::new("ramp");
    rig.tick(256);
    rig.shared.set_playing(true);
    let out = rig.tick(1024);
    // After the 256-frame fade-in the samples are the track itself.
    assert_eq!(out[600 * 2], 600.0);
    assert_eq!(out[1023 * 2], 1023.0);
}

#[test]
fn a_seek_resumes_from_the_new_position() {
    let mut rig = Rig::new("ramp");
    rig.shared.set_playing(true);
    rig.tick(512);
    rig.shared.request_seek(0.5);
    for _ in 0..4 {
        rig.tick(256);
    }
    let out = rig.tick(1024);
    let first = out.iter().step_by(2).copied().find(|&s| s > 0.0).unwrap();
    assert!(first >= 24_000.0, "first sample after the seek = {first}");
    let position = rig.shared.position_seconds();
    assert!((0.5..0.6).contains(&position), "position {position}");
}

#[test]
fn the_end_of_the_track_stops_and_rewinds() {
    let mut rig = Rig::new("short");
    rig.shared.set_playing(true);
    for _ in 0..20 {
        rig.tick(256);
    }
    assert!(!rig.shared.is_playing());
    assert_eq!(rig.shared.position_seconds(), 0.0);
}

#[test]
fn a_different_output_rate_resamples_the_track() {
    let mut rig = Rig::new("cd");
    assert_eq!(rig.shared.duration_seconds(), 1.0);
    rig.shared.set_playing(true);
    let mut played = 0usize;
    for _ in 0..400 {
        let out = rig.tick(256);
        played += out.chunks(2).filter(|frame| frame[0] != 0.0).count();
        if !rig.shared.is_playing() {
            break;
        }
    }
    let frames = played as i64;
    assert!(
        (frames - 48_000).abs() < 600,
        "played {frames} frames at 48 kHz"
    );
}

#[test]
fn a_speed_change_keeps_playing_from_where_it_was() {
    let mut rig = Rig::new("ramp");
    rig.shared.set_playing(true);
    for _ in 0..40 {
        rig.tick(256);
    }
    let before = rig.shared.position_seconds();
    rig.shared.set_settings(PlayerSettings {
        volume: 1.0,
        speed: 0.5,
        ..PlayerSettings::default()
    });
    for _ in 0..4 {
        rig.tick(256);
    }
    let after = rig.shared.position_seconds();
    assert!(
        (after - before).abs() < 0.1,
        "position jumped from {before} to {after}"
    );
    assert!(rig.shared.is_playing());
}

#[test]
fn detaching_keeps_the_heard_position() {
    let mut rig = Rig::new("ramp");
    rig.shared.set_playing(true);
    for _ in 0..40 {
        rig.tick(256);
    }
    let heard = rig.shared.position_seconds();
    rig.worker.handle(PlayerRequest::Detach);
    rig.worker.step();
    assert!((rig.shared.position_seconds() - heard).abs() < 0.02);
}
