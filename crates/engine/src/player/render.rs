//! Responsibility: renders a loaded backing track as the player's settings ask.
//!
//! Runs on the player worker, never on the audio thread. At 1.0× and 0
//! semitones the track is copied untouched; anything else goes through a
//! pitch-preserving time stretcher. An A–B loop joins its end back to its start
//! with a short equal-power crossfade, so the seam never clicks.

use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

use signalsmith_stretch::Stretch;

use super::pcm::PlayerPcm;
use super::settings::PlayerSettings;

/// Length of the crossfade that joins a loop's end to its start.
pub const LOOP_CROSSFADE_SECONDS: f64 = 0.010;

/// Largest block, in frames, the worker asks for in one call. Scratch space is
/// sized for it up front so a steady render never reallocates.
pub const MAX_RENDER_FRAMES: usize = 1024;

pub struct PlayerRenderer {
    pcm: Arc<PlayerPcm>,
    settings: PlayerSettings,
    loop_frames: Option<(usize, usize)>,
    crossfade: usize,
    /// Next source frame the renderer reads.
    pos: usize,
    stretch: Stretch,
    primed: bool,
    /// Fraction of an input frame carried between stretched blocks.
    carry: f64,
    input: Vec<f32>,
    tail_left: Option<usize>,
    ended: bool,
}

impl PlayerRenderer {
    pub fn new(pcm: Arc<PlayerPcm>, settings: PlayerSettings) -> Self {
        let rate = pcm.sample_rate();
        let stretch = Stretch::preset_default(2, rate);
        let crossfade = ((rate as f64 * LOOP_CROSSFADE_SECONDS) as usize).max(1);
        let scratch_frames = MAX_RENDER_FRAMES * 2 + 2 + stretch.input_latency();
        let settings = settings.clamped();
        Self {
            loop_frames: loop_frames(&pcm, settings.loop_range, crossfade),
            pcm,
            settings,
            crossfade,
            pos: 0,
            stretch,
            primed: false,
            carry: 0.0,
            input: Vec::with_capacity(scratch_frames * 2),
            tail_left: None,
            ended: false,
        }
    }

    /// Takes new settings. Returns `true` when what is already queued for the
    /// output no longer matches them (the stretcher switched in or out, or the
    /// play position fell outside a new loop): the caller then empties the
    /// output and seeks to the position being heard.
    pub fn apply(&mut self, settings: PlayerSettings) -> bool {
        let settings = settings.clamped();
        let was_stretching = self.stretching();
        let old_loop = self.loop_frames;
        self.settings = settings;
        self.loop_frames = loop_frames(&self.pcm, settings.loop_range, self.crossfade);
        self.stretch
            .set_transpose_factor_semitones(settings.semitones, None);
        let mode_switched = was_stretching != self.stretching();
        if mode_switched {
            self.primed = false;
        }
        let left_loop = self.loop_frames != old_loop && !self.inside_loop(self.pos);
        mode_switched || left_loop
    }

    /// Jumps to `seconds`; with a loop set, a target outside it lands on the
    /// loop start.
    pub fn seek_seconds(&mut self, seconds: f64) {
        let mut frame = self.pcm.frame_at(seconds);
        if !self.inside_loop(frame) {
            frame = self.loop_frames.map_or(frame, |(start, _)| start);
        }
        self.pos = frame;
        self.primed = false;
        self.carry = 0.0;
        self.tail_left = None;
        self.ended = false;
    }

    pub fn duration_seconds(&self) -> f64 {
        self.pcm.duration_seconds()
    }

    pub fn sample_rate(&self) -> u32 {
        self.pcm.sample_rate()
    }

    /// The track ran out (no loop) and its stretched tail is fully rendered.
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Seconds of the track at the end of the last rendered block, with the
    /// stretcher's own delay taken out.
    pub fn position_seconds(&self) -> f64 {
        let lag = if self.stretching() && self.primed {
            self.stretch.input_latency()
                + (self.stretch.output_latency() as f64 * self.settings.speed as f64) as usize
        } else {
            0
        };
        let mut frame = self.pos as isize - lag as isize;
        if let Some((start, end)) = self.loop_frames {
            if self.pos >= end {
                frame = (start + self.crossfade) as isize - lag as isize;
            }
            if frame < start as isize {
                frame += (end - start) as isize;
            }
        }
        let frame = (frame.max(0) as usize).min(self.pcm.frames());
        frame as f64 / self.pcm.sample_rate() as f64
    }

    /// Fills `out` (interleaved stereo) and returns how many frames it wrote.
    /// Fewer than asked means the track reached its end.
    pub fn render(&mut self, out: &mut [f32]) -> usize {
        let frames = out.len() / 2;
        if self.ended || frames == 0 {
            return 0;
        }
        if self.stretching() {
            self.render_stretched(&mut out[..frames * 2])
        } else {
            self.render_direct(&mut out[..frames * 2])
        }
    }

    fn stretching(&self) -> bool {
        !self.settings.is_unity()
    }

    fn past_end(&self) -> bool {
        self.loop_frames.is_none() && self.pos >= self.pcm.frames()
    }

    fn inside_loop(&self, frame: usize) -> bool {
        self.loop_frames
            .is_none_or(|(start, end)| frame >= start && frame < end)
    }

    fn render_direct(&mut self, out: &mut [f32]) -> usize {
        let mut written = 0;
        for frame in out.chunks_exact_mut(2) {
            if self.past_end() {
                break;
            }
            frame.copy_from_slice(&self.read_frame());
            written += 1;
        }
        if self.past_end() {
            self.ended = true;
        }
        written
    }

    fn render_stretched(&mut self, out: &mut [f32]) -> usize {
        if !self.primed {
            self.prime();
        }
        let frames = out.len() / 2;
        let wanted = self.settings.speed as f64 * frames as f64 + self.carry;
        let input_frames = wanted.floor() as usize;
        self.carry = wanted - input_frames as f64;
        self.input.clear();
        for _ in 0..input_frames {
            let frame = self.read_frame();
            self.input.extend_from_slice(&frame);
        }
        self.stretch.process(&self.input, &mut *out);
        if !self.past_end() {
            return frames;
        }
        let speed = self.settings.speed as f64;
        let tail = self.tail_left.get_or_insert(
            self.stretch.output_latency() + (self.stretch.input_latency() as f64 / speed) as usize,
        );
        if *tail <= frames {
            let written = *tail;
            self.ended = true;
            return written;
        }
        *tail -= frames;
        frames
    }

    /// Restarts the stretcher at the current position, feeding it the input
    /// it needs so the first output is already the track, not a fade-in.
    fn prime(&mut self) {
        self.stretch.reset();
        self.stretch
            .set_transpose_factor_semitones(self.settings.semitones, None);
        self.input.clear();
        for _ in 0..self.stretch.input_latency() {
            let frame = self.read_frame();
            self.input.extend_from_slice(&frame);
        }
        self.stretch.seek(&self.input, self.settings.speed as f64);
        self.primed = true;
        self.carry = 0.0;
    }

    fn read_frame(&mut self) -> [f32; 2] {
        if let Some((start, end)) = self.loop_frames {
            if self.pos >= end {
                let k = self.pos - end;
                let w = (k as f32 + 0.5) / self.crossfade as f32 * FRAC_PI_2;
                let (fade_out, fade_in) = (w.cos(), w.sin());
                let leaving = self.pcm.frame(end + k);
                let entering = self.pcm.frame(start + k);
                self.pos += 1;
                if self.pos - end >= self.crossfade {
                    self.pos = start + self.crossfade;
                }
                return [
                    leaving[0] * fade_out + entering[0] * fade_in,
                    leaving[1] * fade_out + entering[1] * fade_in,
                ];
            }
        }
        let frame = self.pcm.frame(self.pos);
        self.pos += 1;
        frame
    }
}

fn loop_frames(
    pcm: &PlayerPcm,
    range: Option<(f64, f64)>,
    crossfade: usize,
) -> Option<(usize, usize)> {
    let (start, end) = range?;
    let (start, end) = (pcm.frame_at(start), pcm.frame_at(end));
    (end > start + crossfade).then_some((start, end))
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
