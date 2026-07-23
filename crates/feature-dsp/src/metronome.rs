//! Click generator for the built-in metronome (issue #14).
//!
//! Pure DSP: no I/O, no allocation in [`MetronomeGenerator::render`], no
//! knowledge of devices or streams. The dedicated metronome output stream
//! drives it from its audio callback, so every rule of invariant #8 applies.
//!
//! Timing is a beat-phase accumulator advanced PER SAMPLE in `f64`. A tick
//! fires on the exact sample where the phase crosses 1.0 and the fractional
//! remainder carries over, which is what keeps onsets sample-accurate and free
//! of drift whatever buffer size the host hands us.

/// Slowest supported tempo.
pub const BPM_MIN: f32 = 30.0;
/// Fastest supported tempo.
pub const BPM_MAX: f32 = 300.0;

/// Extra clicks between the beats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Subdivision {
    #[default]
    Off,
    Eighths,
    Triplets,
    Sixteenths,
}

impl Subdivision {
    /// How many ticks the generator fires per beat.
    pub fn ticks_per_beat(self) -> u32 {
        match self {
            Subdivision::Off => 1,
            Subdivision::Eighths => 2,
            Subdivision::Triplets => 3,
            Subdivision::Sixteenths => 4,
        }
    }
}

/// Voicing of the click. Synthesized — no samples, no assets, no decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Timbre {
    #[default]
    Click,
    Wood,
    Beep,
}

impl Timbre {
    /// `(beat Hz, downbeat Hz, count-in Hz, decay seconds)`.
    fn voicing(self) -> (f32, f32, f32, f32) {
        match self {
            Timbre::Click => (1000.0, 1600.0, 2000.0, 0.025),
            Timbre::Wood => (800.0, 1200.0, 1600.0, 0.040),
            Timbre::Beep => (880.0, 1320.0, 1760.0, 0.080),
        }
    }
}

/// The accent — beat 1 of the bar, and every count-in tick — plays at full
/// level. A plain beat sits below it, so the bar is audible as a shape and not
/// just as a change of pitch.
const ACCENT_GAIN: f32 = 1.0;
/// Level of a plain beat.
const BEAT_GAIN: f32 = 0.75;
/// Level of a subdivision click relative to a plain beat (−8 dB).
const SUBDIVISION_GAIN: f32 = 0.398;

/// Below this the envelope is finished, so an idle generator writes exact
/// zeros instead of a decaying tail that never quite reaches silence.
const ENVELOPE_FLOOR: f32 = 1e-5;

/// Everything the user can set. Carried as one value so a change reaches the
/// audio thread as a single swap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetronomeSettings {
    pub bpm: f32,
    pub beats_per_bar: u32,
    pub subdivision: Subdivision,
    pub timbre: Timbre,
    /// Linear, `0.0..=1.0`.
    pub volume: f32,
    pub count_in: bool,
}

impl Default for MetronomeSettings {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            beats_per_bar: 4,
            subdivision: Subdivision::Off,
            timbre: Timbre::Click,
            volume: 0.7,
            count_in: false,
        }
    }
}

/// Where the generator is in the bar. The UI reads this position rather than a
/// queue of events, so a slow frame can never lose or double a beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BeatPosition {
    pub bar: u32,
    /// Zero-based beat within the bar.
    pub beat: u32,
    /// Zero-based subdivision tick within the beat.
    pub tick: u32,
    pub counting_in: bool,
}

/// One decaying sine. Plain floats only — retriggering allocates nothing.
#[derive(Debug, Clone, Copy, Default)]
struct ClickVoice {
    phase: f32,
    phase_inc: f32,
    env: f32,
    env_dec: f32,
    amp: f32,
}

impl ClickVoice {
    fn trigger(&mut self, freq: f32, amp: f32, decay_s: f32, sample_rate: f32) {
        self.phase = 0.0;
        self.phase_inc = std::f32::consts::TAU * freq / sample_rate;
        self.env = 1.0;
        // Per-sample factor that takes the envelope to the floor in `decay_s`.
        self.env_dec = (ENVELOPE_FLOOR.ln() / (decay_s * sample_rate)).exp();
        self.amp = amp;
    }

    fn next_sample(&mut self) -> f32 {
        if self.env < ENVELOPE_FLOOR {
            self.env = 0.0;
            return 0.0;
        }
        let s = self.phase.sin() * self.env * self.amp;
        self.phase += self.phase_inc;
        if self.phase > std::f32::consts::TAU {
            self.phase -= std::f32::consts::TAU;
        }
        self.env *= self.env_dec;
        s
    }
}

/// Sample-accurate metronome click generator.
pub struct MetronomeGenerator {
    sample_rate: f32,
    settings: MetronomeSettings,
    /// Position inside the current tick, `0.0..1.0`. Advanced per sample.
    phase: f64,
    /// How much `phase` advances per sample.
    phase_inc: f64,
    /// Ticks played since the last restart, excluding the count-in bar.
    tick_index: u64,
    /// Count-in ticks still to play.
    count_in_left: u32,
    /// Round-robin so a retrigger never truncates the previous click.
    voices: [ClickVoice; 2],
    next_voice: usize,
    position: BeatPosition,
}

impl MetronomeGenerator {
    pub fn new(sample_rate: f32, settings: MetronomeSettings) -> Self {
        let mut generator = Self {
            sample_rate,
            settings,
            phase: 1.0,
            phase_inc: 0.0,
            tick_index: 0,
            count_in_left: 0,
            voices: [ClickVoice::default(); 2],
            next_voice: 0,
            position: BeatPosition::default(),
        };
        generator.recompute_rate();
        generator.restart();
        generator
    }

    fn recompute_rate(&mut self) {
        let bpm = self.settings.bpm.clamp(BPM_MIN, BPM_MAX);
        let ticks_per_beat = self.settings.subdivision.ticks_per_beat().max(1) as f64;
        let samples_per_tick = self.sample_rate as f64 * 60.0 / (bpm as f64 * ticks_per_beat);
        self.phase_inc = if samples_per_tick > 0.0 {
            1.0 / samples_per_tick
        } else {
            0.0
        };
    }

    /// Apply new settings live. The phase is DELIBERATELY preserved so a tempo
    /// change neither restarts the bar nor produces a discontinuity; only a
    /// change to the bar structure resets the tick counter.
    pub fn apply(&mut self, settings: MetronomeSettings) {
        let structure_changed = settings.beats_per_bar != self.settings.beats_per_bar
            || settings.subdivision != self.settings.subdivision;
        self.settings = settings;
        self.recompute_rate();
        if structure_changed {
            self.tick_index = 0;
        }
    }

    /// Restart from bar 1, running the count-in bar when it is enabled.
    pub fn restart(&mut self) {
        // Due immediately, so the first sample rendered is a downbeat.
        self.phase = 1.0;
        self.tick_index = 0;
        self.count_in_left = if self.settings.count_in {
            self.settings.beats_per_bar.max(1)
        } else {
            0
        };
        self.position = BeatPosition {
            counting_in: self.count_in_left > 0,
            ..Default::default()
        };
    }

    pub fn position(&self) -> BeatPosition {
        self.position
    }

    /// Fire the click for the tick that just came due.
    fn on_tick(&mut self) {
        let ticks_per_beat = self.settings.subdivision.ticks_per_beat().max(1) as u64;
        let beats_per_bar = self.settings.beats_per_bar.max(1) as u64;
        let (beat_hz, downbeat_hz, count_in_hz, decay) = self.settings.timbre.voicing();

        let (freq, gain) = if self.count_in_left > 0 {
            // The count-in bar is beats only — subdivisions would blur the very
            // thing a count-off exists to make clear.
            (count_in_hz, ACCENT_GAIN)
        } else {
            let tick = self.tick_index % ticks_per_beat;
            let beat = self.tick_index / ticks_per_beat % beats_per_bar;
            if tick != 0 {
                (beat_hz, BEAT_GAIN * SUBDIVISION_GAIN)
            } else if beat == 0 {
                (downbeat_hz, ACCENT_GAIN)
            } else {
                (beat_hz, BEAT_GAIN)
            }
        };

        let voice = self.next_voice;
        self.voices[voice].trigger(freq, gain, decay, self.sample_rate);
        self.next_voice = (voice + 1) % self.voices.len();

        // Publish AFTER voicing, so `position()` describes the tick the
        // listener is hearing rather than the next one.
        if self.count_in_left > 0 {
            let played = self.settings.beats_per_bar.max(1) - self.count_in_left;
            self.position = BeatPosition {
                bar: 0,
                beat: played,
                tick: 0,
                counting_in: true,
            };
            self.count_in_left -= 1;
        } else {
            self.position = BeatPosition {
                bar: (self.tick_index / (ticks_per_beat * beats_per_bar)) as u32,
                beat: (self.tick_index / ticks_per_beat % beats_per_bar) as u32,
                tick: (self.tick_index % ticks_per_beat) as u32,
                counting_in: false,
            };
            self.tick_index += 1;
        }
    }

    /// Render one mono block. Overwrites `out`. Zero allocation, no locks —
    /// safe to call straight from an audio callback (invariant #8).
    pub fn render(&mut self, out: &mut [f32]) {
        let volume = self.settings.volume.clamp(0.0, 1.0);
        for sample in out.iter_mut() {
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                self.on_tick();
            }
            let mut mixed = 0.0;
            for voice in self.voices.iter_mut() {
                mixed += voice.next_sample();
            }
            *sample = mixed * volume;
            self.phase += self.phase_inc;
        }
    }
}

#[cfg(test)]
#[path = "metronome_tests.rs"]
mod tests;
