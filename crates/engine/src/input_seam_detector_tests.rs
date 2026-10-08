use super::*;

const RATE: f32 = 44_100.0;
const BUFFER: usize = 64;

/// 1.5 s of the rig's In 1 (HD 8, 44.1 kHz, OpenRig at 64 frames) recorded on
/// 30/09 while the chain was ON and the owner heard the stacked "box of bees"
/// sound: per-phase |3rd difference| folded at 64 frames reads max/median 107–186
/// in every 0.25 s window.
const STEPPED: &[u8] = include_bytes!("../tests/fixtures/issue_979/in1_stepped.f32");
/// Same input, same minute, chain OFF: the fold reads 1.1–1.3.
const CLEAN: &[u8] = include_bytes!("../tests/fixtures/issue_979/in1_clean.f32");

fn samples(raw: &[u8]) -> Vec<f32> {
    raw.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// Feeds `signal` in `BUFFER`-frame buffers (mono, like one input channel) and
/// returns the frame count at which the detector first reported a trip.
fn feed(detector: &mut InputSeamDetector, signal: &[f32]) -> Option<usize> {
    let mut fed = 0;
    for chunk in signal.chunks_exact(BUFFER) {
        fed += chunk.len();
        if detector.push(chunk, 1, 0) {
            return Some(fed);
        }
    }
    None
}

fn noise(frames: usize, amplitude: f32) -> Vec<f32> {
    let mut state = 0x2545_F491_u32;
    (0..frames)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * amplitude
        })
        .collect()
}

/// One 0.25 s window of `BUFFER`-frame buffers: the frames after which a
/// window closes.
fn window() -> usize {
    (RATE * 0.25) as usize / BUFFER * BUFFER + BUFFER
}

#[test]
fn the_recorded_stepped_input_trips_after_half_a_second() {
    let mut detector = InputSeamDetector::new(RATE);
    let tripped_at = feed(&mut detector, &samples(STEPPED));
    let frames = tripped_at.expect("the stepped In 1 recording never tripped the detector");
    assert!(
        frames >= RATE as usize / 2,
        "tripped after {frames} frames, before half a second of stepped input"
    );
    assert!(
        frames <= 2 * window(),
        "tripped after {frames} frames, later than two stepped windows"
    );
}

/// `noise` plus a spike at `position` of every buffer, one position per
/// window: the seam a stale driver ring leaves sits at one position for as
/// long as it lasts.
fn seams_at(positions: &[usize]) -> Vec<f32> {
    let mut signal = noise(positions.len() * window(), 0.01);
    for (w, &position) in positions.iter().enumerate() {
        for buffer in signal[w * window()..(w + 1) * window()].chunks_exact_mut(BUFFER) {
            buffer[position] += 0.2;
        }
    }
    signal
}

#[test]
fn a_seam_held_at_one_position_trips() {
    let mut detector = InputSeamDetector::new(RATE);
    assert!(feed(&mut detector, &seams_at(&[10, 10])).is_some());
}

#[test]
fn stepped_windows_whose_seam_moves_never_trip() {
    let mut detector = InputSeamDetector::new(RATE);
    assert_eq!(
        feed(&mut detector, &seams_at(&[10, 40, 10, 40, 10, 40])),
        None
    );
}

#[test]
fn the_recorded_clean_input_never_trips() {
    let mut detector = InputSeamDetector::new(RATE);
    assert_eq!(feed(&mut detector, &samples(CLEAN)), None);
}

/// Feeds `signal` in `BUFFER`-frame buffers and returns the last verdict.
fn verdict_after(detector: &mut InputSeamDetector, signal: &[f32]) -> bool {
    signal
        .chunks_exact(BUFFER)
        .fold(false, |_, chunk| detector.push(chunk, 1, 0))
}

#[test]
fn a_trip_holds_through_less_than_one_second_of_clean_input() {
    let mut detector = InputSeamDetector::new(RATE);
    feed(&mut detector, &samples(STEPPED)).expect("stepped input must trip");
    let part = (RATE as usize * 3 / 4) / BUFFER * BUFFER;
    assert!(
        verdict_after(&mut detector, &samples(CLEAN)[..part]),
        "0.75 s of clean input cleared the trip"
    );
}

/// The app test of #979 (two chains on BlackHole): with the trip latched, a
/// chain whose steps had already stopped was restarted 24 s later anyway. The
/// verdict must say what the input is NOW.
#[test]
fn a_trip_clears_after_one_second_of_clean_input() {
    let mut detector = InputSeamDetector::new(RATE);
    feed(&mut detector, &samples(STEPPED)).expect("stepped input must trip");
    assert!(
        !verdict_after(&mut detector, &samples(CLEAN)),
        "1.5 s of clean input left the trip standing"
    );
}

#[test]
fn reset_clears_a_trip() {
    let mut detector = InputSeamDetector::new(RATE);
    feed(&mut detector, &samples(STEPPED)).expect("stepped input must trip");
    detector.reset();
    assert_eq!(feed(&mut detector, &samples(CLEAN)), None);
}

#[test]
fn white_noise_never_trips() {
    let mut detector = InputSeamDetector::new(RATE);
    assert_eq!(feed(&mut detector, &noise(3 * RATE as usize, 0.01)), None);
}

#[test]
fn digital_silence_never_trips() {
    let mut detector = InputSeamDetector::new(RATE);
    assert_eq!(feed(&mut detector, &vec![0.0; 3 * RATE as usize]), None);
}

#[test]
fn one_stepped_window_between_clean_ones_does_not_trip() {
    let stepped = samples(STEPPED);
    let clean = samples(CLEAN);
    let mut signal = stepped[..window()].to_vec();
    signal.extend_from_slice(&clean[..window()]);
    signal.extend_from_slice(&stepped[window()..2 * window()]);
    let mut detector = InputSeamDetector::new(RATE);
    assert_eq!(feed(&mut detector, &signal), None);
}

#[test]
fn silence_holds_the_count_instead_of_clearing_it() {
    let stepped = samples(STEPPED);
    let part = (RATE as usize * 3 / 4) / BUFFER * BUFFER;
    let mut signal = stepped[..part].to_vec();
    signal.extend(std::iter::repeat_n(0.0, part));
    signal.extend_from_slice(&stepped[part..2 * part]);
    let mut detector = InputSeamDetector::new(RATE);
    assert!(feed(&mut detector, &signal).is_some());
}

#[test]
fn a_buffer_size_change_restarts_the_count() {
    let stepped = samples(STEPPED);
    let part = window();
    let mut detector = InputSeamDetector::new(RATE);
    assert_eq!(feed(&mut detector, &stepped[..part]), None);
    for chunk in stepped[part..].chunks_exact(128).take(8) {
        assert!(!detector.push(chunk, 1, 0));
    }
    let rest = &stepped[part + 8 * 128..];
    let tripped_at = feed(&mut detector, rest);
    assert!(
        tripped_at.is_none_or(|frames| frames >= RATE as usize / 2),
        "the count survived a buffer size change: tripped {tripped_at:?} frames after it"
    );
}

/// A 196 Hz tone at -20 dBFS over the recorded clean floor with every fourth
/// buffer lost (a busy processing lock): each loss is a jump at the start of
/// the next buffer, which folds exactly like a seam unless the detector is told
/// the stream broke there.
#[test]
fn lost_buffers_flagged_as_discontinuities_never_trip() {
    let tone: Vec<f32> = samples(CLEAN)
        .iter()
        .enumerate()
        .map(|(i, floor)| floor + 0.1 * (std::f32::consts::TAU * 196.0 * i as f32 / RATE).sin())
        .collect();
    let mut detector = InputSeamDetector::new(RATE);
    for (k, chunk) in tone.chunks_exact(BUFFER).enumerate() {
        if k % 4 == 3 {
            detector.discontinuity();
            continue;
        }
        assert!(
            !detector.push(chunk, 1, 0),
            "tripped on lost buffers at buffer {k}"
        );
    }
}

#[test]
fn only_the_stepped_channel_of_an_interleaved_buffer_trips() {
    let stepped = samples(STEPPED);
    let clean = samples(CLEAN);
    let interleaved: Vec<f32> = stepped
        .iter()
        .zip(&clean)
        .flat_map(|(s, c)| [*c, *s])
        .collect();
    let mut on_clean = InputSeamDetector::new(RATE);
    let mut on_stepped = InputSeamDetector::new(RATE);
    let (mut clean_tripped, mut stepped_tripped) = (false, false);
    for chunk in interleaved.chunks_exact(BUFFER * 2) {
        clean_tripped |= on_clean.push(chunk, 2, 0);
        stepped_tripped |= on_stepped.push(chunk, 2, 1);
    }
    assert!(!clean_tripped, "the clean channel tripped");
    assert!(stepped_tripped, "the stepped channel did not trip");
}
