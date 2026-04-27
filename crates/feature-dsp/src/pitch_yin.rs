//! YIN-based pitch detection — extracted for reuse beyond the chromatic_tuner block.
//!
//! Runs entirely on a non-RT thread. The audio thread feeds buffers via a bounded
//! channel and drops them when detection is busy; this module never reads from RT.
//!
//! Behavior matches the original implementation embedded in `native_tuner_chromatic`:
//! YIN difference + cumulative mean normalised difference, octave check, parabolic
//! interpolation, smoothing (EMA + snap on >1-semitone jumps), and a silence
//! timeout that clears the display after `SILENCE_TIMEOUT_ROUNDS` consecutive
//! buffers below the RMS threshold.

/// Buffer size fed to the detector — ≈85 ms @ 48 kHz, reliable down to A1 (55 Hz).
pub const BUFFER_SIZE: usize = 4096;

/// Minimum samples required to attempt detection.
const MIN_DETECTION: usize = 2048;
const MIN_FREQ: f32 = 55.0; // A1
const MAX_FREQ: f32 = 1200.0;
/// RMS below which we treat the buffer as silence and skip detection.
/// Tuned for instrument-level inputs read pre-FX from the audio tap —
/// a clean guitar plucked softly sits around 0.005-0.02 RMS, so we set
/// the floor low enough to keep detecting on quiet decay but high
/// enough that idle hum (~0.0005) does not trigger false detections.
const RMS_SILENCE_THRESHOLD: f32 = 0.0015;
/// YIN difference threshold: candidates below this are "good enough" without
/// looking for a global minimum. Slightly more permissive than 0.15 so the
/// detector picks up notes earlier on the attack envelope.
const YIN_ABSOLUTE_THRESHOLD: f32 = 0.20;
/// YIN absolute fallback: if no candidate beat the absolute threshold but the
/// global minimum is below this, accept it. More lax than the original 0.4
/// to reduce missed detections on noisy/quiet signals.
const YIN_REJECT_THRESHOLD: f32 = 0.55;

const EMA_ALPHA: f32 = 0.25;
const SNAP_RATIO: f32 = 1.06; // ~one semitone — snap on large jumps
const DEBOUNCE_COUNT: u32 = 1;

/// Consecutive silent buffers before the display is cleared. At BUFFER_SIZE / 48 kHz
/// per round (~85 ms), 12 rounds ≈ 1 s of sustained silence.
const SILENCE_TIMEOUT_ROUNDS: usize = 12;

/// Default reference frequency for A4. Configurable per-instance.
pub const DEFAULT_REFERENCE_HZ: f32 = 440.0;

const NOTES: [&str; 12] = [
    "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
];

/// Result of feeding one buffer to the detector.
#[derive(Debug, Clone, PartialEq)]
pub enum PitchUpdate {
    /// New value to show — display the given note, cents offset, and frequency.
    Update {
        note: &'static str,
        cents: f32,
        freq: f32,
    },
    /// Sustained silence has elapsed — clear the display.
    Silence,
    /// Pitch unstable or short silence — caller should keep the previous value.
    NoChange,
}

/// Convert a frequency to the nearest note name and cents offset, given a reference
/// (typically 440 Hz for A4 but configurable, e.g. 432 Hz, 442 Hz).
pub fn freq_to_note(frequency: f32, reference_hz: f32) -> (&'static str, f32) {
    let semitones_from_a4 = 12.0 * (frequency / reference_hz).log2();
    let note_number = semitones_from_a4.round() as i32;
    let cents = (semitones_from_a4 - note_number as f32) * 100.0;
    let note_index = note_number.rem_euclid(12) as usize;
    (NOTES[note_index], cents)
}

/// Stateful pitch detector. Feed buffers of mono samples sized [`BUFFER_SIZE`]
/// (or anything ≥ `MIN_DETECTION` samples) and consume [`PitchUpdate`] values.
pub struct PitchDetector {
    sample_rate: usize,
    reference_hz: f32,
    smoothed_freq: Option<f32>,
    current_note: Option<&'static str>,
    pending_note: Option<&'static str>,
    pending_count: u32,
    silent_rounds: usize,
}

impl PitchDetector {
    pub fn new(sample_rate: usize, reference_hz: f32) -> Self {
        Self {
            sample_rate,
            reference_hz,
            smoothed_freq: None,
            current_note: None,
            pending_note: None,
            pending_count: 0,
            silent_rounds: 0,
        }
    }

    /// Recommended buffer size for callers — `BUFFER_SIZE` mono samples per call.
    pub const fn buffer_size() -> usize {
        BUFFER_SIZE
    }

    /// Process one buffer of mono samples and return what the UI should do with it.
    pub fn process_buffer(&mut self, buf: &[f32]) -> PitchUpdate {
        match self.detect_pitch(buf) {
            Some(raw_freq) => {
                self.silent_rounds = 0;
                let freq = self.smooth_frequency(raw_freq);
                let (note_name, cents) = freq_to_note(freq, self.reference_hz);
                self.debounce_note(note_name);
                match self.current_note {
                    Some(current) => PitchUpdate::Update {
                        note: current,
                        cents,
                        freq,
                    },
                    None => PitchUpdate::NoChange,
                }
            }
            None => {
                self.silent_rounds += 1;
                if self.silent_rounds >= SILENCE_TIMEOUT_ROUNDS {
                    self.smoothed_freq = None;
                    self.current_note = None;
                    self.pending_note = None;
                    self.pending_count = 0;
                    PitchUpdate::Silence
                } else {
                    PitchUpdate::NoChange
                }
            }
        }
    }

    fn detect_pitch(&self, buf: &[f32]) -> Option<f32> {
        let n = buf.len();
        if n < MIN_DETECTION {
            return None;
        }
        let rms = (buf.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();
        if rms < RMS_SILENCE_THRESHOLD {
            return None;
        }
        let min_tau = (self.sample_rate as f32 / MAX_FREQ).ceil() as usize;
        let max_tau = ((self.sample_rate as f32 / MIN_FREQ).floor() as usize).min(n / 2);
        if min_tau >= max_tau {
            return None;
        }
        let d = yin_difference(buf, max_tau + 1);
        let d_prime = yin_cmnd(&d);
        let (best_tau, _) = yin_find_tau(&d_prime, min_tau, max_tau)?;
        let checked_tau = octave_check(&d_prime, best_tau, self.sample_rate);
        let refined_tau = parabolic_interpolation(&d_prime, checked_tau);
        if refined_tau <= 0.0 {
            return None;
        }
        let freq = self.sample_rate as f32 / refined_tau;
        if freq < MIN_FREQ || freq > MAX_FREQ {
            None
        } else {
            Some(freq)
        }
    }

    fn smooth_frequency(&mut self, raw_freq: f32) -> f32 {
        match self.smoothed_freq {
            None => {
                self.smoothed_freq = Some(raw_freq);
                raw_freq
            }
            Some(prev) => {
                let ratio = raw_freq / prev;
                if ratio > SNAP_RATIO || ratio < 1.0 / SNAP_RATIO {
                    self.smoothed_freq = Some(raw_freq);
                    raw_freq
                } else {
                    let smoothed = prev * (1.0 - EMA_ALPHA) + raw_freq * EMA_ALPHA;
                    self.smoothed_freq = Some(smoothed);
                    smoothed
                }
            }
        }
    }

    fn debounce_note(&mut self, note: &'static str) {
        if self.pending_note == Some(note) {
            self.pending_count += 1;
        } else {
            self.pending_note = Some(note);
            self.pending_count = 1;
        }
        if self.pending_count >= DEBOUNCE_COUNT {
            self.current_note = Some(note);
        }
    }
}

fn yin_difference(buf: &[f32], max_tau: usize) -> Vec<f32> {
    let n = buf.len();
    let mut d = vec![0.0_f32; max_tau];
    for tau in 1..max_tau {
        let mut sum = 0.0;
        for i in 0..(n - tau) {
            let diff = buf[i] - buf[i + tau];
            sum += diff * diff;
        }
        d[tau] = sum;
    }
    d
}

fn yin_cmnd(d: &[f32]) -> Vec<f32> {
    let mut d_prime = vec![0.0_f32; d.len()];
    d_prime[0] = 1.0;
    let mut running_sum = 0.0;
    for tau in 1..d.len() {
        running_sum += d[tau];
        if running_sum > 0.0 {
            d_prime[tau] = d[tau] * tau as f32 / running_sum;
        } else {
            d_prime[tau] = 1.0;
        }
    }
    d_prime
}

fn yin_find_tau(d_prime: &[f32], min_tau: usize, max_tau: usize) -> Option<(usize, f32)> {
    let upper = max_tau.min(d_prime.len());
    let mut tau = min_tau;
    while tau < upper {
        if d_prime[tau] < YIN_ABSOLUTE_THRESHOLD {
            while tau + 1 < upper && d_prime[tau + 1] < d_prime[tau] {
                tau += 1;
            }
            return Some((tau, d_prime[tau]));
        }
        tau += 1;
    }
    let mut best_tau = None;
    let mut best_val = f32::MAX;
    for tau in min_tau..upper {
        if d_prime[tau] < best_val {
            best_val = d_prime[tau];
            best_tau = Some(tau);
        }
    }
    if best_val < YIN_REJECT_THRESHOLD {
        best_tau.map(|t| (t, best_val))
    } else {
        None
    }
}

fn parabolic_interpolation(d_prime: &[f32], tau: usize) -> f32 {
    if tau < 1 || tau + 1 >= d_prime.len() {
        return tau as f32;
    }
    let s0 = d_prime[tau - 1];
    let s1 = d_prime[tau];
    let s2 = d_prime[tau + 1];
    let denom = 2.0 * s1 - s2 - s0;
    if denom.abs() < 1e-12 {
        tau as f32
    } else {
        tau as f32 + (s2 - s0) / (2.0 * denom)
    }
}

fn octave_check(d_prime: &[f32], best_tau: usize, sample_rate: usize) -> usize {
    let sub_tau = best_tau * 2;
    if sub_tau >= d_prime.len() {
        return best_tau;
    }
    let sub_freq = sample_rate as f32 / sub_tau as f32;
    if sub_freq < MIN_FREQ {
        return best_tau;
    }
    if d_prime[sub_tau] < d_prime[best_tau] * 1.5 {
        sub_tau
    } else {
        best_tau
    }
}

#[cfg(test)]
mod tests {
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
}
