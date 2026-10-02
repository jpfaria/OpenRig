//! Responsibility: schedules a groove's hits on a sample-accurate beat clock.
//!
//! Time is counted in whole frames since `play`; a beat position maps to the
//! first frame at or after it, so a hit lands on the same sample whatever the
//! buffer size. A tempo change re-anchors the clock at the current position.
//!
//! Fills follow the play-along pedal convention: asked for mid-bar, the fill
//! takes over from the current position to the end of the bar; asked for in
//! the last beat, it plays the whole next bar. The groove resumes after it.

use super::groove::Groove;
use super::pattern::{DrumHit, DrumPattern};

/// Slack when mapping a beat to a frame, so float error never pushes an
/// on-grid hit one frame late.
const FRAME_EPSILON: f64 = 1e-6;
/// Slack when flooring a beat position to its bar or beat number.
const BEAT_EPSILON: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct DrumPosition {
    pub playing: bool,
    pub bar: u32,
    /// Beat inside the bar, from 0.
    pub beat: u32,
    pub in_fill: bool,
}

#[derive(Clone, Copy, Debug)]
struct ActiveFill {
    index: usize,
    start: f64,
    end: f64,
}

/// The next hits to fire: every hit of `pattern` at absolute beat `beat`.
#[derive(Clone, Copy, Debug)]
struct Pending {
    beat: f64,
    frame: u64,
    origin: f64,
    index: usize,
    in_fill: bool,
}

pub(crate) struct Sequencer {
    sample_rate: f64,
    frames_per_beat: f64,
    anchor_frame: u64,
    anchor_beat: f64,
    frame: u64,
    playing: bool,
    pending: Option<Pending>,
    fill: Option<ActiveFill>,
    fill_turn: usize,
}

impl Sequencer {
    pub fn new(sample_rate: f32, bpm: f32) -> Self {
        let sample_rate = f64::from(sample_rate.max(1.0));
        Self {
            sample_rate,
            frames_per_beat: sample_rate * 60.0 / f64::from(bpm),
            anchor_frame: 0,
            anchor_beat: 0.0,
            frame: 0,
            playing: false,
            pending: None,
            fill: None,
            fill_turn: 0,
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Starts from the top of the groove; a no-op while already playing.
    pub fn play(&mut self, groove: &Groove) {
        if self.playing {
            return;
        }
        self.playing = true;
        self.frame = 0;
        self.anchor_frame = 0;
        self.anchor_beat = 0.0;
        self.fill = None;
        self.reschedule(groove);
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.pending = None;
        self.fill = None;
    }

    pub fn set_bpm(&mut self, bpm: f32) {
        let frames_per_beat = self.sample_rate * 60.0 / f64::from(bpm);
        if frames_per_beat == self.frames_per_beat {
            return;
        }
        self.anchor_beat = self.current_beat();
        self.anchor_frame = self.frame;
        self.frames_per_beat = frames_per_beat;
        if let Some(mut pending) = self.pending {
            pending.frame = self.frame_of(pending.beat);
            self.pending = Some(pending);
        }
    }

    /// Keeps the beat position; drops any fill of the previous groove.
    pub fn groove_changed(&mut self, groove: &Groove) {
        self.fill = None;
        if self.playing {
            self.reschedule(groove);
        }
    }

    pub fn trigger_fill(&mut self, groove: &Groove) {
        if !self.playing || groove.fills.is_empty() || self.in_fill() {
            return;
        }
        let beats_per_bar = f64::from(groove.beats_per_bar.max(1));
        let beat = self.current_beat();
        let bar_start = (beat / beats_per_bar + BEAT_EPSILON).floor() * beats_per_bar;
        let start = if beat - bar_start >= beats_per_bar - 1.0 {
            bar_start + beats_per_bar
        } else {
            bar_start
        };
        let index = self.fill_turn % groove.fills.len();
        self.fill_turn = self.fill_turn.wrapping_add(1);
        self.fill = Some(ActiveFill {
            index,
            start,
            end: start + groove.fills[index].beats(),
        });
        self.reschedule(groove);
    }

    /// Frames until the next scheduled hit, if any.
    pub fn frames_to_next_hit(&self) -> Option<u64> {
        self.pending.map(|p| p.frame.saturating_sub(self.frame))
    }

    /// Calls `on_hit` for every hit due at the current frame.
    pub fn fire_due(&mut self, groove: &Groove, mut on_hit: impl FnMut(DrumHit)) {
        while let Some(pending) = self.pending {
            if pending.frame > self.frame {
                break;
            }
            let pattern = self.pattern_of(groove, pending.in_fill);
            let hits = pattern.hits();
            let mut index = pending.index;
            while index < hits.len() && pending.origin + hits[index].beat == pending.beat {
                on_hit(hits[index]);
                index += 1;
            }
            self.pending = self.next_after(groove, pending.beat, false);
        }
    }

    pub fn advance(&mut self, frames: usize) {
        if self.playing {
            self.frame += frames as u64;
        }
    }

    pub fn position(&self, groove: &Groove) -> DrumPosition {
        if !self.playing {
            return DrumPosition::default();
        }
        let beats_per_bar = groove.beats_per_bar.max(1);
        let beat = self.current_beat() + BEAT_EPSILON;
        DrumPosition {
            playing: true,
            bar: (beat / f64::from(beats_per_bar)).floor() as u32,
            beat: (beat.floor() as u64 % u64::from(beats_per_bar)) as u32,
            in_fill: self.in_fill(),
        }
    }

    fn in_fill(&self) -> bool {
        self.fill.is_some_and(|f| self.current_beat() < f.end)
    }

    fn current_beat(&self) -> f64 {
        self.anchor_beat + (self.frame - self.anchor_frame) as f64 / self.frames_per_beat
    }

    /// First frame at or after absolute `beat`, never in the past.
    fn frame_of(&self, beat: f64) -> u64 {
        let offset = ((beat - self.anchor_beat) * self.frames_per_beat - FRAME_EPSILON).ceil();
        let frame = self.anchor_frame as f64 + offset;
        if frame <= self.frame as f64 {
            self.frame
        } else {
            frame as u64
        }
    }

    /// Re-finds the next hit from the current frame: every hit that maps to
    /// this frame or later is still unplayed.
    fn reschedule(&mut self, groove: &Groove) {
        let played_until = self.anchor_beat
            + ((self.frame - self.anchor_frame) as f64 - 1.0 + FRAME_EPSILON)
                / self.frames_per_beat;
        self.pending = self.next_after(groove, played_until, false);
    }

    fn pattern_of<'g>(&self, groove: &'g Groove, in_fill: bool) -> &'g DrumPattern {
        match (in_fill, self.fill) {
            (true, Some(fill)) => &groove.fills[fill.index],
            _ => &groove.beat,
        }
    }

    fn next_after(&self, groove: &Groove, from: f64, inclusive: bool) -> Option<Pending> {
        let found = match self.fill {
            Some(fill) if from < fill.end => {
                let before_fill = if from < fill.start {
                    loop_next(&groove.beat, from, inclusive).filter(|p| p.beat < fill.start)
                } else {
                    None
                };
                before_fill.or_else(|| {
                    let (from, inclusive) = if from < fill.start {
                        (fill.start, true)
                    } else {
                        (from, inclusive)
                    };
                    once_next(&groove.fills[fill.index], fill.start, from, inclusive)
                        .filter(|p| p.beat < fill.end)
                        .or_else(|| loop_next(&groove.beat, fill.end, true))
                })
            }
            _ => loop_next(&groove.beat, from, inclusive),
        };
        found.map(|mut p| {
            p.frame = self.frame_of(p.beat);
            p
        })
    }
}

/// Next hit of `pattern` played once from `origin`.
fn once_next(pattern: &DrumPattern, origin: f64, from: f64, inclusive: bool) -> Option<Pending> {
    let index = pattern.first_from(origin, from, inclusive);
    pattern.hits().get(index).map(|hit| Pending {
        beat: origin + hit.beat,
        frame: 0,
        origin,
        index,
        in_fill: true,
    })
}

/// Next hit of `pattern` looping forever from beat 0.
fn loop_next(pattern: &DrumPattern, from: f64, inclusive: bool) -> Option<Pending> {
    if pattern.hits().is_empty() {
        return None;
    }
    let length = pattern.beats();
    let first_loop = ((from / length).floor().max(0.0) as u64).saturating_sub(1);
    (first_loop..first_loop + 3).find_map(|turn| {
        let origin = turn as f64 * length;
        once_next(pattern, origin, from, inclusive).map(|p| Pending {
            in_fill: false,
            ..p
        })
    })
}
