//! Responsibility: holds the metronome's control-plane state.
//! #127: the metronome's control-plane state, owned by the dispatcher.
//!
//! It used to live in the GUI (`adapter_gui`'s `MetronomeSession`, in what is
//! now `metronome_view.rs`), which is why
//! a footswitch or an MCP client could flip `metronome_enabled` and hear
//! nothing: only the GUI's own knob callbacks knew how to turn an event into
//! sound, and only the GUI held the settings to start with. Moving the state
//! here makes the command the whole answer — validated, remembered, applied,
//! persisted — for every transport.
//!
//! **Layering.** [`MetronomeSettings`] is `feature_dsp::metronome`'s value
//! type: six scalars, no DSP state, no device knowledge. `application` already
//! depends on `feature-dsp` (the tuner's pitch reference, the spectrum's band
//! table, the Tone Doctor's symptom limits), and the alternative — a second
//! settings struct here with a second copy of `Subdivision`/`Timbre` — would
//! put the enum's vocabulary in two places and need a conversion in both
//! directions. The value type stays in the deepest crate that defines it and
//! everyone above shares it.
//!
//! **No devices.** The chosen output travels as an opaque endpoint KEY. This
//! crate never learns which device or which channels that is — the frontend
//! that owns the audio host resolves it (see [`crate::live_source`]).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use feature_dsp::metronome::{MetronomeSettings, Subdivision, Timbre, BPM_MAX, BPM_MIN};
use infra_filesystem::MetronomeConfig;

/// A gap longer than this is a fresh count-off, not a very slow tap.
const TAP_RESET: Duration = Duration::from_secs(2);
/// How many intervals the tap average spans.
const TAP_WINDOW: usize = 4;

/// Average of the last taps, ignoring gaps above 2 s (a new count-off).
/// `None` until there are at least two usable taps.
pub fn tap_bpm(intervals: &[Duration]) -> Option<f32> {
    // Everything up to and including the last long gap belongs to a count the
    // player already abandoned.
    let fresh = intervals
        .iter()
        .rposition(|gap| *gap > TAP_RESET)
        .map_or(intervals, |last_gap| &intervals[last_gap + 1..]);
    let window = &fresh[fresh.len().saturating_sub(TAP_WINDOW)..];
    if window.is_empty() {
        return None;
    }
    let mean = window.iter().map(Duration::as_secs_f32).sum::<f32>() / window.len() as f32;
    if mean <= 0.0 {
        return None;
    }
    Some((60.0 / mean).clamp(BPM_MIN, BPM_MAX))
}

/// What the metronome is set to, and whether it is meant to be sounding.
///
/// This is the shape a frontend renders and a transport reads. It is a value,
/// not a handle: taking one never borrows the dispatcher across a UI call.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MetronomeSnapshot {
    pub settings: MetronomeSettings,
    /// The chosen output endpoint key, `None` for "the project's first".
    pub output_key: Option<String>,
    /// Whether POWER is on. Not persisted — the app always boots silent.
    pub running: bool,
}

/// The dispatcher's metronome state: the snapshot plus the tap history that
/// produces a tempo.
///
/// The history lives here, not in a frontend, so a footswitch bound to the tap
/// counts off the same beat the on-screen TAP button does.
#[derive(Debug, Default)]
pub struct MetronomeControlState {
    snapshot: MetronomeSnapshot,
    last_tap: Option<Instant>,
    intervals: Vec<Duration>,
    /// The per-machine `config.yaml` these settings live in (ADR 0003). The
    /// state knows its own home so a handler never has to guess one.
    ///
    /// `None` ⇒ **nothing is persisted**, and deliberately so: a dispatcher
    /// that no frontend handed a config path (every test, a headless
    /// transport) must never write the user's real config — that recurrence
    /// is issue #701.
    config_path: Option<PathBuf>,
}

impl MetronomeControlState {
    /// The state a frontend restores at boot: the persisted settings, and the
    /// file they came from.
    pub fn restored(config: &MetronomeConfig, config_path: Option<PathBuf>) -> Self {
        let mut state = Self {
            config_path,
            ..Self::default()
        };
        state.seed_from_config(config);
        state
    }

    /// Where these settings persist, if anywhere.
    pub fn config_path(&self) -> Option<PathBuf> {
        self.config_path.clone()
    }

    /// Restore the settings from the per-machine config. `running` stays
    /// false: `MetronomeConfig` has no `enabled` field precisely so no code
    /// path can make a session start clicking on its own.
    ///
    /// Unknown enum keys fall back to the default value rather than failing —
    /// a hand-edited or older `config.yaml` must still open the app. A command
    /// carrying an unknown key is a different matter and is rejected.
    pub fn seed_from_config(&mut self, config: &MetronomeConfig) {
        self.snapshot.settings = MetronomeSettings {
            bpm: config.bpm.clamp(BPM_MIN, BPM_MAX),
            beats_per_bar: config.beats_per_bar,
            subdivision: Subdivision::from_key(&config.subdivision).unwrap_or_default(),
            timbre: Timbre::from_key(&config.timbre).unwrap_or_default(),
            volume: config.volume.clamp(0.0, 1.0),
            count_in: config.count_in,
        };
        self.snapshot.output_key = config.output_device.clone();
    }

    pub fn snapshot(&self) -> MetronomeSnapshot {
        self.snapshot.clone()
    }

    pub fn settings(&self) -> MetronomeSettings {
        self.snapshot.settings
    }

    pub fn output_key(&self) -> Option<&str> {
        self.snapshot.output_key.as_deref()
    }

    pub fn running(&self) -> bool {
        self.snapshot.running
    }

    pub fn set_running(&mut self, running: bool) {
        self.snapshot.running = running;
    }

    pub fn set_output_key(&mut self, key: Option<String>) {
        self.snapshot.output_key = key;
    }

    /// Edit the settings in place and hand back the new value, so a caller
    /// applies exactly what was stored.
    pub fn update_settings(
        &mut self,
        edit: impl FnOnce(&mut MetronomeSettings),
    ) -> MetronomeSettings {
        edit(&mut self.snapshot.settings);
        self.snapshot.settings
    }

    /// Record a tap and return the tempo it implies, if any. `now` is a
    /// parameter so the history is testable without sleeping.
    pub fn tap_at(&mut self, now: Instant) -> Option<f32> {
        if let Some(previous) = self.last_tap {
            self.intervals.push(now.duration_since(previous));
            // Only the tail of the history can ever affect the average; keeping
            // it short means a long practice session does not grow a vector
            // forever.
            if self.intervals.len() > TAP_WINDOW * 2 {
                self.intervals.drain(..self.intervals.len() - TAP_WINDOW);
            }
        }
        self.last_tap = Some(now);
        tap_bpm(&self.intervals)
    }
}

#[cfg(test)]
#[path = "metronome_state_tests.rs"]
mod tests;
