//! Scale definitions and target note selection for pitch correction.

/// Scale intervals as semitones from root.
const SCALE_MAJOR: &[u8] = &[0, 2, 4, 5, 7, 9, 11];
const SCALE_NATURAL_MINOR: &[u8] = &[0, 2, 3, 5, 7, 8, 10];
const SCALE_PENTATONIC_MAJOR: &[u8] = &[0, 2, 4, 7, 9];
const SCALE_PENTATONIC_MINOR: &[u8] = &[0, 3, 5, 7, 10];
const SCALE_HARMONIC_MINOR: &[u8] = &[0, 2, 3, 5, 7, 8, 11];
const SCALE_MELODIC_MINOR: &[u8] = &[0, 2, 3, 5, 7, 9, 11];
const SCALE_BLUES: &[u8] = &[0, 3, 5, 6, 7, 10];
const SCALE_DORIAN: &[u8] = &[0, 2, 3, 5, 7, 9, 10];

const ALL_SCALES: [&[u8]; 8] = [
    SCALE_MAJOR,
    SCALE_NATURAL_MINOR,
    SCALE_PENTATONIC_MAJOR,
    SCALE_PENTATONIC_MINOR,
    SCALE_HARMONIC_MINOR,
    SCALE_MELODIC_MINOR,
    SCALE_BLUES,
    SCALE_DORIAN,
];

/// Convert a key string to its numeric index (0-11).
pub fn key_from_str(s: &str) -> u8 {
    match s {
        "c" => 0, "cs" => 1, "d" => 2, "ds" => 3,
        "e" => 4, "f" => 5, "fs" => 6, "g" => 7,
        "gs" => 8, "a" => 9, "as" => 10, "b" => 11,
        _ => 0,
    }
}

/// Convert a scale string to its numeric index (0-7).
pub fn scale_from_str(s: &str) -> u8 {
    match s {
        "major" => 0, "natural_minor" => 1,
        "pentatonic_major" => 2, "pentatonic_minor" => 3,
        "harmonic_minor" => 4, "melodic_minor" => 5,
        "blues" => 6, "dorian" => 7,
        _ => 0,
    }
}

/// Convert frequency to continuous MIDI note number.
fn freq_to_midi(freq: f32) -> f32 {
    12.0 * (freq / 440.0).log2() + 69.0
}

/// Convert MIDI note number to frequency.
fn midi_to_freq(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

/// Snap a frequency to the nearest chromatic semitone.
pub fn nearest_chromatic(freq: f32) -> f32 {
    let midi = freq_to_midi(freq);
    let target_midi = midi.round();
    midi_to_freq(target_midi)
}

/// Snap a frequency to the nearest note in a given key and scale.
///
/// `key`: 0=C, 1=C#, 2=D, ..., 11=B
/// `scale`: 0=Major, 1=Natural Minor, ..., 7=Dorian
pub fn nearest_in_scale(freq: f32, key: u8, scale: u8) -> f32 {
    let scale_index = (scale as usize).min(ALL_SCALES.len() - 1);
    let intervals = ALL_SCALES[scale_index];
    let key = key % 12;

    let midi = freq_to_midi(freq);
    let midi_rounded = midi.round() as i32;

    // Search nearby semitones for the closest scale degree
    let mut best_midi = midi_rounded;
    let mut best_distance = f32::MAX;

    for offset in -12..=12 {
        let candidate = midi_rounded + offset;
        let note_in_octave = candidate.rem_euclid(12) as u8;
        // Check if this note is in the scale (relative to key)
        let degree = (note_in_octave + 12 - key) % 12;
        if intervals.contains(&degree) {
            let distance = (midi - candidate as f32).abs();
            if distance < best_distance {
                best_distance = distance;
                best_midi = candidate;
            }
        }
    }

    midi_to_freq(best_midi as f32)
}

/// Apply a detune offset in cents to a frequency.
pub fn apply_detune(freq: f32, cents: f32) -> f32 {
    freq * 2f32.powf(cents / 1200.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromatic_snaps_445_to_440() {
        let target = nearest_chromatic(445.0);
        let error = (target - 440.0).abs();
        assert!(error < 0.01, "expected ~440Hz, got {target}Hz");
    }

    #[test]
    fn chromatic_snaps_exact() {
        let target = nearest_chromatic(440.0);
        let error = (target - 440.0).abs();
        assert!(error < 0.01);
    }

    #[test]
    fn scale_c_major_snaps_to_nearest() {
        // F#4 = MIDI 66, not in C major. Should snap to F4 (65) or G4 (67).
        let f_sharp = midi_to_freq(66.0); // ~369.99
        let target = nearest_in_scale(f_sharp, 0, 0); // C major

        let f4_freq = midi_to_freq(65.0);
        let g4_freq = midi_to_freq(67.0);
        assert!(
            (target - f4_freq).abs() < 0.01 || (target - g4_freq).abs() < 0.01,
            "expected F4 or G4, got {target}Hz"
        );
    }

    #[test]
    fn scale_in_scale_note_unchanged() {
        // C4 = MIDI 60, in C major
        let c4 = midi_to_freq(60.0);
        let target = nearest_in_scale(c4, 0, 0);
        let error = (target - c4).abs();
        assert!(error < 0.01, "C4 should stay as C4, got {target}Hz");
    }

    #[test]
    fn detune_positive_raises_pitch() {
        let base = 440.0;
        let detuned = apply_detune(base, 100.0); // +100 cents = 1 semitone up
        let expected = 440.0 * 2f32.powf(100.0 / 1200.0);
        let error = (detuned - expected).abs();
        assert!(error < 0.01);
    }

    #[test]
    fn detune_zero_unchanged() {
        let base = 440.0;
        let detuned = apply_detune(base, 0.0);
        assert!((detuned - base).abs() < 0.001);
    }
}
