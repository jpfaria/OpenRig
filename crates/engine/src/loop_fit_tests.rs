use super::*;
use crate::loop_edit::{content_bounds, SEAM_FRAMES};

const RATE: u32 = 48_000;

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

    fn gauss(&mut self) -> f64 {
        self.uni() + self.uni() + self.uni()
    }
}

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

/// An eight-note riff at 63 bpm (7.619 s a pass), played `reps` times after
/// `lead` seconds of count-in silence and followed by `tail` seconds of room,
/// with human timing and touch over a faint hiss. Interleaved stereo; returns
/// the take and one pass in frames.
fn take(reps: f64, lead: f64, tail: f64, seed: u64) -> (Vec<f32>, f64) {
    const NOTES: [f64; 8] = [110.0, 165.0, 220.0, 247.0, 147.0, 196.0, 247.0, 294.0];
    let r = RATE as f64;
    let mut rng = Rng::new(seed);
    let beat = 60.0 / 63.0;
    let period = 8.0 * beat;
    let mut y = vec![0.0f64; ((lead + reps * period + tail) * r) as usize];
    for b in 0..(reps * 8.0) as usize {
        let at = (lead + b as f64 * beat + 0.008 * rng.gauss()).max(0.0);
        let note = pluck(NOTES[b % 8], beat * 2.0, r, 0.3 * (1.0 + 0.1 * rng.gauss()));
        for (dst, src) in y.iter_mut().skip((at * r) as usize).zip(&note) {
            *dst += src;
        }
    }
    for s in y.iter_mut() {
        *s += 0.0005 * rng.uni();
    }
    let pcm = y
        .iter()
        .flat_map(|&s| [s as f32, (s * 0.9) as f32])
        .collect();
    (pcm, period * r)
}

/// Within 50 ms of `passes` whole passes: the playing itself is no tighter.
fn assert_loops_on(region: (usize, usize), passes: f64, period: f64) {
    let (start, end) = region;
    // The kept region carries the seam's overlap past the loop's end.
    let length = (end - start - SEAM_FRAMES) as f64;
    assert!(
        (length - passes * period).abs() < 0.05 * RATE as f64,
        "kept {:.4} s, {passes} passes are {:.4} s",
        length / RATE as f64,
        passes * period / RATE as f64
    );
}

#[test]
fn fit_starts_at_the_first_attack_and_keeps_whole_passes() {
    // 2.6 passes after a count-in: the count-in goes, and so does the
    // unfinished third pass, so the loop's end meets its start.
    let (pcm, period) = take(2.6, 1.3, 1.0, 7);

    let region = fit_region(&pcm, RATE).expect("the riff repeats");

    assert_eq!(region.0, content_bounds(&pcm).unwrap().0);
    assert_loops_on(region, 2.0, period);
}

#[test]
fn fit_keeps_only_the_passes_the_take_holds() {
    // Almost two passes, but the take stops before the second one is whole.
    let (pcm, period) = take(1.9, 0.8, 0.2, 9);

    let region = fit_region(&pcm, RATE).expect("the riff repeats");

    assert_loops_on(region, 1.0, period);
    assert!(region.1 <= pcm.len() / 2);
}

#[test]
fn fit_finds_nothing_to_loop_in_silence() {
    assert_eq!(fit_region(&vec![0.0; RATE as usize * 20], RATE), None);
}
