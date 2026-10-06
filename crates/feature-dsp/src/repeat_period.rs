//! Responsibility: measures how long the repeating pattern of a recording is.
//!
//! A loop base is a riff or a progression played over and over, and a loop
//! only sounds right when it is cut to whole passes of it. This finds the
//! length of one pass from the audio alone.
//!
//! The recording is cut into ~12 ms frames, each described two ways: its
//! loudness in log-spaced bands (rhythm and timbre) and the energy of its 12
//! pitch classes (harmony). When the playing repeats, the descriptions repeat,
//! so their autocorrelation peaks at one pass and at every multiple of it. Of
//! the clear peaks the shortest wins — a riff also peaks at two passes — as
//! long as it is nearly as strong as the best one (half of a riff whose halves
//! differ peaks only weakly), and the multiples it lines up with refine it
//! below the frame size.
//!
//! Control-thread work: it allocates and runs FFTs over the whole recording.

use rustfft::{num_complex::Complex, FftPlanner};

/// Analysis frames per second (~12 ms each).
const FRAMES_PER_SECOND: f64 = 86.0;
/// Lowest frequency analysed: below it is rumble, not playing.
const LOW_HZ: f64 = 70.0;
/// Highest frequency analysed: above it is pick noise and hiss.
const HIGH_HZ: f64 = 5_000.0;
/// Log-spaced loudness bands between [`LOW_HZ`] and [`HIGH_HZ`].
const BANDS: usize = 40;
const PITCH_CLASSES: usize = 12;
/// Scale of the band loudness inside its logarithm: soft notes still count,
/// loud ones do not drown the rest.
const LOUDNESS_GAIN: f64 = 1_000.0;
/// Below this fraction of the loudest frame, a frame's pitch content is faded
/// out: the harmony of a near-silent frame is noise.
const PITCH_FLOOR: f64 = 0.05;
/// Shortest pass looked for: anything shorter is a beat, not a pattern.
const MIN_PASS_SECONDS: f64 = 1.0;
/// The recording must overlap itself by at least this long at a lag…
const MIN_OVERLAP_SECONDS: f64 = 1.5;
/// …and by at least this fraction of its length, for the lag to be trusted.
const MIN_OVERLAP_DIVISOR: usize = 6;
/// Peaks closer than this are the same peak.
const PEAK_RADIUS_SECONDS: f64 = 0.1;
/// The best peak must be at least this alike for the playing to count as
/// repeating at all.
const MIN_SIMILARITY: f64 = 0.35;
/// A shorter peak wins over the best one when it is at least this close to it.
const NEAR_BEST: f64 = 0.85;
/// How far, in frames, a multiple of the pass may sit from its exact lag.
const MULTIPLE_SLACK: usize = 2;

/// One pass of the pattern `mono` repeats, in samples — `None` when nothing
/// repeats clearly enough to say.
pub fn repeat_period(mono: &[f32], sample_rate: u32) -> Option<f64> {
    let rate = f64::from(sample_rate.max(1));
    let hop = ((rate / FRAMES_PER_SECOND).round() as usize).max(1);
    let size = (hop * 8).next_power_of_two();
    if mono.len() < size + hop * 10 {
        return None;
    }
    let (bands, pitch) = describe(mono, rate, hop, size);
    let similarity: Vec<f64> = self_similarity(&bands, BANDS)
        .iter()
        .zip(self_similarity(&pitch, PITCH_CLASSES))
        .map(|(a, b)| 0.5 * (a + b))
        .collect();

    let frames_per_second = rate / hop as f64;
    let frames = similarity.len();
    let overlap =
        ((MIN_OVERLAP_SECONDS * frames_per_second) as usize).max(frames / MIN_OVERLAP_DIVISOR);
    let lags = (MIN_PASS_SECONDS * frames_per_second) as usize..=frames.checked_sub(overlap)?;
    if lags.is_empty() {
        return None;
    }
    let radius = ((PEAK_RADIUS_SECONDS * frames_per_second) as usize).max(2);
    let pass = shortest_strong_peak(&similarity, &lags, radius)?;
    Some(refine(&similarity, pass, &lags) * hop as f64)
}

/// Per analysis frame: the log loudness of each band, and the pitch-class
/// profile (unit length, faded out on near-silent frames). Row-major.
fn describe(mono: &[f32], rate: f64, hop: usize, size: usize) -> (Vec<f64>, Vec<f64>) {
    let frames = 1 + (mono.len() - size) / hop;
    let fft = FftPlanner::<f64>::new().plan_fft_forward(size);
    let mut scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
    let mut buf = vec![Complex::default(); size];
    let window: Vec<f64> = (0..size)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (size - 1) as f64).cos())
        .collect();
    let (band_of, class_of) = bin_maps(rate, size);

    let mut bands = vec![0.0f64; frames * BANDS];
    let mut pitch = vec![0.0f64; frames * PITCH_CLASSES];
    let mut loudest_bin = 0.0f64;
    for t in 0..frames {
        let samples = &mono[t * hop..t * hop + size];
        for ((c, &s), w) in buf.iter_mut().zip(samples).zip(&window) {
            *c = Complex::new(f64::from(s) * w, 0.0);
        }
        fft.process_with_scratch(&mut buf, &mut scratch);
        for (k, c) in buf[..=size / 2].iter().enumerate() {
            let magnitude = c.norm();
            loudest_bin = loudest_bin.max(magnitude);
            if let Some(b) = band_of[k] {
                bands[t * BANDS + b] += magnitude;
            }
            if let Some(p) = class_of[k] {
                pitch[t * PITCH_CLASSES + p] += magnitude;
            }
        }
    }

    for b in bands.iter_mut() {
        *b = (LOUDNESS_GAIN * *b / (loudest_bin + 1e-12)).ln_1p();
    }
    let strength: Vec<f64> = pitch
        .chunks_exact(PITCH_CLASSES)
        .map(|p| p.iter().map(|v| v * v).sum::<f64>().sqrt())
        .collect();
    let strongest = strength.iter().fold(0.0f64, |a, &s| a.max(s));
    for (p, &s) in pitch.chunks_exact_mut(PITCH_CLASSES).zip(&strength) {
        let fade = (s / (strongest + 1e-12) / PITCH_FLOOR).min(1.0);
        for v in p.iter_mut() {
            *v = *v / (s + 1e-12) * fade;
        }
    }
    (bands, pitch)
}

/// The loudness band and the pitch class each FFT bin feeds, if any.
fn bin_maps(rate: f64, size: usize) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
    let span = (HIGH_HZ / LOW_HZ).ln();
    (0..=size / 2)
        .map(|k| {
            let hz = k as f64 * rate / size as f64;
            let band = (LOW_HZ..HIGH_HZ)
                .contains(&hz)
                .then(|| (((hz / LOW_HZ).ln() / span * BANDS as f64) as usize).min(BANDS - 1));
            let class = (LOW_HZ..=HIGH_HZ).contains(&hz).then(|| {
                (12.0 * (hz / 440.0).log2()).round().rem_euclid(12.0) as usize % PITCH_CLASSES
            });
            (band, class)
        })
        .unzip()
}

/// How alike a sequence of `dims`-wide feature frames is to itself shifted by
/// each lag: the autocorrelation of the mean-removed features, normalised by
/// the energy of the two stretches that overlap (1 = identical).
fn self_similarity(features: &[f64], dims: usize) -> Vec<f64> {
    let frames = features.len() / dims;
    let size = (2 * frames).next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let forward = planner.plan_fft_forward(size);
    let inverse = planner.plan_fft_inverse(size);
    let mut buf = vec![Complex::default(); size];
    let mut power = vec![Complex::default(); size];
    let mut energy = vec![0.0f64; frames + 1];
    for d in 0..dims {
        let column = || features.iter().skip(d).step_by(dims);
        let mean = column().sum::<f64>() / frames as f64;
        buf.fill(Complex::default());
        for (t, &v) in column().enumerate() {
            buf[t].re = v - mean;
            energy[t + 1] += (v - mean) * (v - mean);
        }
        forward.process(&mut buf);
        for (p, c) in power.iter_mut().zip(&buf) {
            p.re += c.norm_sqr();
        }
    }
    inverse.process(&mut power);
    for t in 0..frames {
        energy[t + 1] += energy[t];
    }
    (0..frames)
        .map(|lag| {
            let head = energy[frames - lag];
            let tail = energy[frames] - energy[lag];
            power[lag].re / size as f64 / (head * tail + 1e-12).sqrt()
        })
        .collect()
}

/// The shortest lag that is a local peak nearly as alike as the best peak —
/// `None` when even the best peak is not alike enough to call a repeat.
fn shortest_strong_peak(
    s: &[f64],
    lags: &std::ops::RangeInclusive<usize>,
    radius: usize,
) -> Option<usize> {
    let peaks: Vec<usize> = lags
        .clone()
        .filter(|&l| {
            let from = (*lags.start()).max(l.saturating_sub(radius));
            let to = (l + radius).min(s.len() - 1);
            s[from..=to].iter().all(|&v| v <= s[l])
        })
        .collect();
    let best = peaks.iter().map(|&l| s[l]).fold(f64::MIN, f64::max);
    if best < MIN_SIMILARITY {
        return None;
    }
    peaks.into_iter().find(|&l| s[l] >= NEAR_BEST * best)
}

/// The pass length, in fractional frames, that best fits the peaks at `pass`
/// and its multiples: each peak is placed between frames by a parabola, and a
/// line through the origin is fitted to them.
fn refine(s: &[f64], pass: usize, lags: &std::ops::RangeInclusive<usize>) -> f64 {
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for m in 1..=*lags.end() / pass {
        let from = (*lags.start()).max((m * pass).saturating_sub(MULTIPLE_SLACK));
        let to = (*lags.end()).min(m * pass + MULTIPLE_SLACK);
        if from > to {
            continue;
        }
        let j = (from..=to).fold(from, |b, l| if s[l] > s[b] { l } else { b });
        if j == 0 || j + 1 >= s.len() || s[j] < NEAR_BEST * s[pass] {
            continue;
        }
        let (y0, y1, y2) = (s[j - 1], s[j], s[j + 1]);
        let curve = y0 - 2.0 * y1 + y2;
        let offset = if curve != 0.0 {
            0.5 * (y0 - y2) / curve
        } else {
            0.0
        };
        num += m as f64 * (j as f64 + offset);
        den += (m * m) as f64;
    }
    if den > 0.0 {
        num / den
    } else {
        pass as f64
    }
}

#[cfg(test)]
#[path = "repeat_period_tests.rs"]
mod tests;
