//! Responsibility: detects a stepped (buffer-seam) pattern on one received input channel.
//!
//! #979: when the input arrives broken, every received buffer carries a step at
//! the same position — the "box of bees" buzz at `rate / buffer` Hz. The measure
//! is the mean |3rd difference| of the signal per position inside the buffer,
//! folded over a 0.25 s window: a clean input reads max/median ≈ 1.0–1.3, the
//! recorded broken In 1 reads 107–186. Two consecutive stepped windows (0.5 s)
//! with signal present and the step at the same buffer position trip the
//! detector — every window of buzz is heard, and a seam left by the driver
//! stays put while a loud transient lands anywhere — and four consecutive
//! clean ones (1 s) clear the trip, so the verdict is what the input is now,
//! never a stale latch; a window without signal holds both counts, a buffer
//! size change restarts them.
//!
//! Real-time safe: `push` never allocates, locks or blocks.

const MAX_PERIOD: usize = 2048;
const WINDOW_SECONDS: f32 = 0.25;
const TRIP_WINDOWS: u32 = 2;
const CLEAR_WINDOWS: u32 = 4;
const STEP_RATIO: f32 = 5.0;
/// Positions the peak of two windows may differ by and still be one seam: the
/// |3rd difference| of a step spreads over the four positions after it.
const SEAM_TOLERANCE: usize = 2;
/// Mean square of -90 dBFS: below it the window carries no signal to judge.
const SIGNAL_GATE: f64 = 1e-9;

pub struct InputSeamDetector {
    window_frames: usize,
    period: usize,
    phase_sums: Box<[f32]>,
    scratch: Box<[f32]>,
    history: [f32; 3],
    history_len: usize,
    window_fed: usize,
    energy: f64,
    stepped_windows: u32,
    seam_position: usize,
    clean_windows: u32,
    tripped: bool,
}

impl InputSeamDetector {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            window_frames: ((sample_rate * WINDOW_SECONDS) as usize).max(1),
            period: 0,
            phase_sums: vec![0.0; MAX_PERIOD].into_boxed_slice(),
            scratch: vec![0.0; MAX_PERIOD].into_boxed_slice(),
            history: [0.0; 3],
            history_len: 0,
            window_fed: 0,
            energy: 0.0,
            stepped_windows: 0,
            seam_position: 0,
            clean_windows: 0,
            tripped: false,
        }
    }

    /// Feeds one received interleaved buffer and reads `channel` from it.
    /// Returns true while the channel is stepped: from half a second of
    /// stepped input until one second of clean input.
    pub fn push(&mut self, data: &[f32], channels: usize, channel: usize) -> bool {
        if channels == 0 || channel >= channels {
            return self.tripped;
        }
        let frames = data.len() / channels;
        if frames == 0 {
            return self.tripped;
        }
        if frames != self.period {
            self.restart(frames);
        }
        if self.period > MAX_PERIOD {
            return self.tripped;
        }
        for (phase, frame) in data.chunks_exact(channels).enumerate() {
            let x = frame[channel];
            if self.history_len == 3 {
                let [a, b, c] = self.history;
                self.phase_sums[phase] += (x - 3.0 * c + 3.0 * b - a).abs();
            } else {
                self.history_len += 1;
            }
            self.history = [self.history[1], self.history[2], x];
            self.energy += f64::from(x) * f64::from(x);
        }
        self.window_fed += frames;
        if self.window_fed >= self.window_frames {
            self.close_window();
        }
        self.tripped
    }

    pub fn is_tripped(&self) -> bool {
        self.tripped
    }

    /// The next buffer does not follow the last one (a buffer was lost).
    pub fn discontinuity(&mut self) {
        self.history_len = 0;
    }

    pub fn reset(&mut self) {
        self.restart(0);
        self.tripped = false;
    }

    fn restart(&mut self, period: usize) {
        self.period = period;
        self.phase_sums.fill(0.0);
        self.history_len = 0;
        self.window_fed = 0;
        self.energy = 0.0;
        self.stepped_windows = 0;
        self.clean_windows = 0;
    }

    fn close_window(&mut self) {
        let mean_square = self.energy / self.window_fed as f64;
        if mean_square >= SIGNAL_GATE {
            match self.window_seam() {
                Some(position) => {
                    let same_seam = self.stepped_windows > 0
                        && self.same_position(position, self.seam_position);
                    self.stepped_windows = if same_seam {
                        self.stepped_windows + 1
                    } else {
                        1
                    };
                    self.seam_position = position;
                    self.clean_windows = 0;
                }
                None => {
                    self.stepped_windows = 0;
                    self.clean_windows += 1;
                }
            }
            if self.stepped_windows >= TRIP_WINDOWS {
                self.tripped = true;
            } else if self.clean_windows >= CLEAR_WINDOWS {
                self.tripped = false;
            }
        }
        self.phase_sums[..self.period].fill(0.0);
        self.window_fed = 0;
        self.energy = 0.0;
    }

    /// The buffer position of the step when the window is stepped.
    fn window_seam(&mut self) -> Option<usize> {
        let (position, max) = self.phase_sums[..self.period]
            .iter()
            .copied()
            .enumerate()
            .fold(
                (0, 0.0_f32),
                |peak, (i, sum)| if sum > peak.1 { (i, sum) } else { peak },
            );
        let sums = &mut self.scratch[..self.period];
        sums.copy_from_slice(&self.phase_sums[..self.period]);
        let middle = sums.len() / 2;
        let (_, median, _) = sums.select_nth_unstable_by(middle, f32::total_cmp);
        (*median > 0.0 && max > STEP_RATIO * *median).then_some(position)
    }

    /// Whether two positions inside the buffer are the same seam, around the
    /// buffer's end.
    fn same_position(&self, a: usize, b: usize) -> bool {
        let distance = a.abs_diff(b);
        distance.min(self.period - distance) <= SEAM_TOLERANCE
    }
}

#[cfg(test)]
#[path = "input_seam_detector_tests.rs"]
mod tests;
