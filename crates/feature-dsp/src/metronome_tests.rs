//! Issue #14 — the metronome's timing contract.
//!
//! These tests render offline and assert on sample positions, so they need no
//! audio device and they pin the one property the feature lives or dies by:
//! a click lands exactly on the beat, at any rate, under any buffer size.

use super::*;

/// Detects where clicks start, carrying its state across buffers.
///
/// The generator writes EXACT zeros between clicks, so a long enough run of
/// zeros ends a click and the next non-zero sample starts the next one. The run
/// has to outlast one sine period, otherwise the zero crossings inside a click
/// would each read as a new onset. Keeping the state in a struct is what lets a
/// chunked render be analysed as one continuous signal.
struct OnsetDetector {
    in_click: bool,
    zeros: usize,
    consumed: usize,
}

impl OnsetDetector {
    const SILENCE_RUN: usize = 128;

    fn new() -> Self {
        Self {
            in_click: false,
            zeros: Self::SILENCE_RUN,
            consumed: 0,
        }
    }

    /// Absolute sample indices (across every buffer fed so far) where a click
    /// starts in `buf`.
    fn feed(&mut self, buf: &[f32]) -> Vec<usize> {
        let mut out = Vec::new();
        for (i, &s) in buf.iter().enumerate() {
            if s == 0.0 {
                self.zeros += 1;
                if self.zeros >= Self::SILENCE_RUN {
                    self.in_click = false;
                }
            } else {
                if !self.in_click && self.zeros >= Self::SILENCE_RUN {
                    out.push(self.consumed + i);
                    self.in_click = true;
                }
                self.zeros = 0;
            }
        }
        self.consumed += buf.len();
        out
    }
}

/// Onsets of a signal rendered in one go.
fn onsets(buf: &[f32]) -> Vec<usize> {
    OnsetDetector::new().feed(buf)
}

/// Peak magnitude of the click that starts at `start`, up to the next onset.
fn peak_from(buf: &[f32], start: usize, end: usize) -> f32 {
    buf[start..end.min(buf.len())]
        .iter()
        .fold(0.0f32, |m, s| m.max(s.abs()))
}

#[test]
fn onsets_land_on_the_beat_at_any_rate() {
    for &rate in &[44_100.0f32, 48_000.0] {
        let settings = MetronomeSettings {
            bpm: 120.0,
            ..Default::default()
        };
        let mut generator = MetronomeGenerator::new(rate, settings);
        // 4 s at 120 bpm = 8 beats.
        let mut buf = vec![0.0f32; (rate * 4.0) as usize];
        generator.render(&mut buf);

        let expected: Vec<usize> = (0..8).map(|b| (b as f32 * rate * 0.5) as usize).collect();
        let got = onsets(&buf);
        assert_eq!(got.len(), expected.len(), "beat count at {rate} Hz");
        for (g, e) in got.iter().zip(&expected) {
            assert!(
                (*g as i64 - *e as i64).abs() <= 1,
                "onset {g} should be within 1 sample of {e} at {rate} Hz"
            );
        }
    }
}

#[test]
fn no_drift_over_ten_minutes() {
    let rate = 44_100.0f32;
    let mut generator = MetronomeGenerator::new(
        rate,
        MetronomeSettings {
            bpm: 120.0,
            ..Default::default()
        },
    );
    // Render in chunks so the ten minutes do not need one huge allocation.
    let chunk = 4096usize;
    let total = (rate as usize) * 600;
    let mut buf = vec![0.0f32; chunk];
    let mut detector = OnsetDetector::new();
    let mut last_onset = 0usize;
    let mut rendered = 0usize;
    let mut count = 0usize;
    while rendered < total {
        generator.render(&mut buf);
        for o in detector.feed(&buf) {
            last_onset = o;
            count += 1;
        }
        rendered += chunk;
    }
    // Beat period at 120 bpm is half a second.
    let ideal = (count - 1) as f64 * rate as f64 * 0.5;
    assert!(
        (last_onset as f64 - ideal).abs() <= 1.0,
        "after 10 minutes the last onset was at {last_onset}, ideal {ideal}"
    );
}

#[test]
fn callback_size_does_not_move_onsets() {
    let rate = 48_000.0f32;
    let settings = MetronomeSettings {
        bpm: 137.0,
        ..Default::default()
    };
    let total = (rate * 4.0) as usize;

    let mut reference = vec![0.0f32; total];
    MetronomeGenerator::new(rate, settings).render(&mut reference);
    let expected = onsets(&reference);

    // 480 and 1023 are deliberately not divisors of the beat period.
    for &block in &[64usize, 128, 512, 480, 1023] {
        let mut generator = MetronomeGenerator::new(rate, settings);
        let mut out = vec![0.0f32; total];
        for chunk in out.chunks_mut(block) {
            generator.render(chunk);
        }
        assert_eq!(onsets(&out), expected, "block size {block} moved the onsets");
    }
}

#[test]
fn downbeat_is_louder_than_beat() {
    let rate = 48_000.0f32;
    let mut generator = MetronomeGenerator::new(rate, MetronomeSettings::default());
    let mut buf = vec![0.0f32; (rate * 2.0) as usize];
    generator.render(&mut buf);

    let marks = onsets(&buf);
    assert!(marks.len() >= 2, "need at least two clicks");
    let downbeat = peak_from(&buf, marks[0], marks[1]);
    let beat = peak_from(&buf, marks[1], marks.get(2).copied().unwrap_or(buf.len()));
    assert!(
        downbeat > beat * 1.05,
        "downbeat peak {downbeat} should exceed beat peak {beat}"
    );
}

#[test]
fn subdivision_is_eight_db_below_the_beat() {
    let rate = 48_000.0f32;
    let mut generator = MetronomeGenerator::new(
        rate,
        MetronomeSettings {
            bpm: 120.0,
            subdivision: Subdivision::Eighths,
            ..Default::default()
        },
    );
    let mut buf = vec![0.0f32; (rate * 2.0) as usize];
    generator.render(&mut buf);

    let marks = onsets(&buf);
    assert!(marks.len() >= 4, "expected beats and their subdivisions");
    // marks[1] is the off-beat between downbeat and beat 2.
    let beat = peak_from(&buf, marks[2], marks[3]);
    let sub = peak_from(&buf, marks[1], marks[2]);
    let ratio = sub / beat;
    assert!(
        (ratio - 0.398).abs() < 0.05,
        "subdivision/beat ratio was {ratio}, expected ~0.398 (-8 dB)"
    );
}

#[test]
fn beats_per_bar_drives_the_accent() {
    let rate = 48_000.0f32;
    let mut generator = MetronomeGenerator::new(
        rate,
        MetronomeSettings {
            bpm: 240.0,
            beats_per_bar: 3,
            ..Default::default()
        },
    );
    let mut buf = vec![0.0f32; (rate * 3.0) as usize];
    generator.render(&mut buf);

    let marks = onsets(&buf);
    assert!(marks.len() >= 7, "need at least two full bars");
    let peaks: Vec<f32> = marks
        .windows(2)
        .map(|w| peak_from(&buf, w[0], w[1]))
        .collect();
    // Accents at 0, 3, 6 — every third click is the loudest of its group.
    for bar_start in [0usize, 3] {
        assert!(
            peaks[bar_start] > peaks[bar_start + 1] * 1.05,
            "click {bar_start} should be the accent of its bar"
        );
        assert!(
            peaks[bar_start] > peaks[bar_start + 2] * 1.05,
            "click {bar_start} should be the accent of its bar"
        );
    }
}

#[test]
fn count_in_adds_exactly_one_bar() {
    let rate = 48_000.0f32;
    let mut generator = MetronomeGenerator::new(
        rate,
        MetronomeSettings {
            bpm: 240.0,
            beats_per_bar: 4,
            count_in: true,
            ..Default::default()
        },
    );
    // One tick at a time so the position can be sampled per click.
    let samples_per_beat = (rate * 60.0 / 240.0) as usize;
    let mut buf = vec![0.0f32; samples_per_beat];
    let mut counting = 0;
    for _ in 0..8 {
        generator.render(&mut buf);
        if generator.position().counting_in {
            counting += 1;
        }
    }
    assert_eq!(counting, 4, "count-in should cover exactly one 4-beat bar");
}

#[test]
fn live_bpm_change_has_no_discontinuity() {
    let rate = 48_000.0f32;
    let mut generator = MetronomeGenerator::new(
        rate,
        MetronomeSettings {
            bpm: 100.0,
            ..Default::default()
        },
    );
    let mut first = vec![0.0f32; 24_000];
    generator.render(&mut first);
    generator.apply(MetronomeSettings {
        bpm: 180.0,
        ..Default::default()
    });
    let mut second = vec![0.0f32; 24_000];
    generator.render(&mut second);

    let joined: Vec<f32> = first.iter().chain(second.iter()).copied().collect();
    for w in joined.windows(2) {
        assert!(
            (w[1] - w[0]).abs() < 0.5,
            "sample-to-sample jump of {} after a live tempo change",
            (w[1] - w[0]).abs()
        );
    }
}

#[test]
fn volume_scales_the_output() {
    let rate = 48_000.0f32;
    let peak_at = |volume: f32| {
        let mut generator = MetronomeGenerator::new(
            rate,
            MetronomeSettings {
                volume,
                ..Default::default()
            },
        );
        let mut buf = vec![0.0f32; 12_000];
        generator.render(&mut buf);
        buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    };
    let full = peak_at(1.0);
    let half = peak_at(0.5);
    assert!(full > 0.0, "a click should have been rendered");
    assert!(
        (half / full - 0.5).abs() < 0.01,
        "halving the volume should halve the peak (got {half} vs {full})"
    );
}

#[test]
fn render_is_silent_between_clicks() {
    let rate = 48_000.0f32;
    let mut generator = MetronomeGenerator::new(
        rate,
        MetronomeSettings {
            bpm: 60.0,
            ..Default::default()
        },
    );
    let mut buf = vec![0.0f32; 48_000];
    generator.render(&mut buf);
    // A 25 ms click at 60 bpm leaves most of the second in exact silence.
    let tail = &buf[24_000..];
    assert!(
        tail.iter().all(|s| *s == 0.0),
        "the gap between clicks must be exact zeros"
    );
}
