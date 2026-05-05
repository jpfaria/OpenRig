use super::*;

fn sine_wave(freq: f32, sample_rate: usize, num_samples: usize) -> Vec<f32> {
    (0..num_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (2.0 * std::f32::consts::PI * freq * t).sin()
        })
        .collect()
}

#[test]
fn freq_to_note_a4_is_a_zero_cents() {
    let (note, cents) = freq_to_note(440.0, 440.0);
    assert_eq!(note, "A");
    assert!(cents.abs() < 0.01, "Expected ~0 cents, got {cents}");
}

#[test]
fn freq_to_note_a4_with_432_reference_is_a() {
    let (note, _) = freq_to_note(432.0, 432.0);
    assert_eq!(note, "A");
}

#[test]
fn detect_a4_440hz() {
    let mut detector = PitchDetector::new(44100, 440.0);
    let samples = sine_wave(440.0, 44100, BUFFER_SIZE);
    match detector.process_buffer(&samples) {
        PitchUpdate::Update { note, .. } => assert_eq!(note, "A"),
        other => panic!("Expected Update with A, got {other:?}"),
    }
}

#[test]
fn detect_e_low_82hz() {
    let mut detector = PitchDetector::new(44100, 440.0);
    let samples = sine_wave(82.41, 44100, BUFFER_SIZE);
    match detector.process_buffer(&samples) {
        PitchUpdate::Update { note, .. } => assert_eq!(note, "E"),
        other => panic!("Expected Update with E, got {other:?}"),
    }
}

#[test]
fn silence_is_no_change_until_timeout() {
    let mut detector = PitchDetector::new(44100, 440.0);
    let silent = vec![0.0_f32; BUFFER_SIZE];
    // Below SILENCE_TIMEOUT_ROUNDS calls → NoChange
    for _ in 0..(SILENCE_TIMEOUT_ROUNDS - 1) {
        assert_eq!(detector.process_buffer(&silent), PitchUpdate::NoChange);
    }
    // Reaching the threshold triggers Silence
    assert_eq!(detector.process_buffer(&silent), PitchUpdate::Silence);
}

#[test]
fn buffer_smaller_than_min_returns_no_change() {
    let mut detector = PitchDetector::new(44100, 440.0);
    let small = vec![0.5_f32; MIN_DETECTION - 1];
    // First call doesn't have enough samples → no detection, but still counts
    // as "silent round" because detect_pitch returns None — so NoChange (until timeout).
    assert_eq!(detector.process_buffer(&small), PitchUpdate::NoChange);
}
