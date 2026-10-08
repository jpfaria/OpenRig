use super::*;

/// Deterministic xorshift noise, so every take is the same on every machine.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        let x = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        Self(x.max(1))
    }

    /// Uniform in [-1, 1).
    fn uni(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }

    /// Roughly normal, unit-ish spread.
    fn gauss(&mut self) -> f64 {
        self.uni() + self.uni() + self.uni()
    }
}

/// A plucked string: six decaying harmonics with a 30-sample attack.
fn pluck(f0: f64, secs: f64, rate: f64, amp: f64) -> Vec<f64> {
    (0..(secs * rate) as usize)
        .map(|i| {
            let t = i as f64 / rate;
            let tone: f64 = (0..6)
                .map(|h| 0.6f64.powi(h) * (std::f64::consts::TAU * f0 * (h + 1) as f64 * t).sin())
                .sum();
            amp * tone * (-3.0 * t).exp() * (t * rate / 30.0).min(1.0)
        })
        .collect()
}

fn mix_in(y: &mut [f64], at_secs: f64, rate: f64, note: &[f64]) {
    let i = (at_secs * rate) as usize;
    for (dst, src) in y.iter_mut().skip(i).zip(note) {
        *dst += src;
    }
}

/// The bar's notes: the second half differs from the first, so half a bar is
/// not the pattern.
const NOTES: [f64; 8] = [110.0, 165.0, 220.0, 247.0, 147.0, 196.0, 247.0, 294.0];

/// A riff of `beats` notes at `bpm`, played `reps` times from the very first
/// sample, with human timing (±8 ms) and touch, over a faint hiss. Returns the
/// take and the riff's true length in seconds.
fn riff(rate: u32, bpm: f64, beats: usize, reps: f64, seed: u64) -> (Vec<f32>, f64) {
    let r = rate as f64;
    let mut rng = Rng::new(seed);
    let beat = 60.0 / bpm;
    let period = beats as f64 * beat;
    let mut y = vec![0.0f64; ((reps * period + 1.0) * r) as usize];
    for b in 0..(reps * beats as f64) as usize {
        let at = (b as f64 * beat + 0.008 * rng.gauss()).max(0.0);
        let amp = 0.3 * (1.0 + 0.1 * rng.gauss());
        mix_in(&mut y, at, r, &pluck(NOTES[b % beats], beat * 2.0, r, amp));
    }
    for s in y.iter_mut() {
        *s += 0.0005 * rng.uni();
    }
    (y.into_iter().map(|s| s as f32).collect(), period)
}

/// Notes at random pitches and random times: playing that never repeats.
fn wander(rate: u32, secs: f64, seed: u64) -> Vec<f32> {
    let r = rate as f64;
    let mut rng = Rng::new(seed);
    let mut y = vec![0.0f64; (secs * r) as usize];
    let mut at = 0.3;
    while at < secs - 0.5 {
        let f0 = 110.0 * 2f64.powf(rng.uni() * 1.5 + 1.5);
        mix_in(&mut y, at, r, &pluck(f0, 1.2, r, 0.3));
        at += 0.25 + 0.35 * (rng.uni() + 1.0);
    }
    for s in y.iter_mut() {
        *s += 0.0005 * rng.uni();
    }
    y.into_iter().map(|s| s as f32).collect()
}

/// A human-played riff is never on a grid: within 25 ms of the written length
/// is as close as the playing itself.
fn assert_period(found: Option<f64>, rate: u32, truth_secs: f64) {
    let found = found.expect("the riff repeats") / rate as f64;
    assert!(
        (found - truth_secs).abs() < 0.025,
        "found {found:.4} s, the riff is {truth_secs:.4} s"
    );
}

#[test]
fn finds_one_pass_of_a_riff_played_two_and_a_half_times() {
    // Half a bar is not a pass: its two halves differ.
    let (take, period) = riff(48_000, 63.0, 8, 2.6, 7);
    assert_period(repeat_period(&take, 48_000), 48_000, period);
}

#[test]
fn finds_one_pass_not_two_when_the_riff_repeats_many_times() {
    let (take, period) = riff(44_100, 120.0, 8, 3.3, 11);
    assert_period(repeat_period(&take, 44_100), 44_100, period);
}

#[test]
fn finds_a_riff_played_less_than_twice() {
    let (take, period) = riff(48_000, 63.0, 8, 1.4, 5);
    assert_period(repeat_period(&take, 48_000), 48_000, period);
}

#[test]
fn playing_that_never_repeats_has_no_period() {
    for seed in [1, 2] {
        assert_eq!(repeat_period(&wander(48_000, 20.0, seed), 48_000), None);
    }
}

#[test]
fn silence_has_no_period() {
    assert_eq!(repeat_period(&vec![0.0; 480_000], 48_000), None);
}

#[test]
fn a_take_too_short_to_hold_a_repeat_has_no_period() {
    let (take, _) = riff(48_000, 120.0, 4, 1.0, 3);
    assert_eq!(repeat_period(&take[..72_000], 48_000), None);
}
