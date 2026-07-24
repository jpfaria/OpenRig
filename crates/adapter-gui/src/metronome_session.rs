//! Metronome UI session (#14) — the settings the window renders, the output
//! device it plays through, and the tap-tempo history.
//!
//! This is the adapter's copy of the metronome state. The audio side keeps its
//! own lock-free copy in [`engine::metronome_state::MetronomeShared`]; this one
//! exists because the window has to render (and `config.yaml` has to persist)
//! the settings even while no runtime controller is alive — between projects,
//! or before the first one is opened.
//!
//! It also owns the index↔key translation for the three knob controls. The
//! knobs speak indices, the `Command`s speak strings (`"eighths"`, `"wood"`),
//! and every conversion between the two lives here so no call site has to
//! remember the order of a list.
//!
//! The beat lamps are NOT driven from here: the wiring's timer reads
//! `MetronomeShared::position()` — a phase, not a queue of events — so a slow
//! frame can never lose or double a beat.

use std::time::{Duration, Instant};

use engine::metronome_state::{MetronomeSettings, Subdivision, Timbre};
use infra_filesystem::MetronomeConfig;

pub use engine::metronome_state::{BPM_MAX, BPM_MIN};

/// A gap longer than this is a fresh count-off, not a very slow tap.
const TAP_RESET: Duration = Duration::from_secs(2);
/// How many intervals the tap average spans.
const TAP_WINDOW: usize = 4;

/// The knob's seven time signatures: `(beats per bar, label)`. The beat count
/// is the numerator — what the accent and the lamps follow.
const TIME_SIGNATURES: [(u32, &str); 7] = [
    (2, "2/4"),
    (3, "3/4"),
    (4, "4/4"),
    (5, "5/4"),
    (6, "6/8"),
    (7, "7/8"),
    (12, "12/8"),
];
/// 4/4 — where the knob rests until the user moves it.
const DEFAULT_TIME_SIGNATURE_INDEX: i32 = 2;

/// Subdivision knob positions: `(command key, label)`. The labels are note
/// values, which read the same in every language.
const SUBDIVISIONS: [(&str, &str); 4] = [
    ("off", "1/4"),
    ("eighths", "1/8"),
    ("triplets", "1/8T"),
    ("sixteenths", "1/16"),
];

/// Timbre knob positions: `(command key, translation key)`.
const TIMBRES: [(&str, &str); 3] = [
    ("click", "label-metronome-timbre-click"),
    ("wood", "label-metronome-timbre-wood"),
    ("beep", "label-metronome-timbre-beep"),
];

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

/// Index of `key` in `table`, or `None` when the key is unknown.
fn index_of(table: &[(&'static str, &'static str)], key: &str) -> Option<i32> {
    table
        .iter()
        .position(|(k, _)| *k == key)
        .map(|index| index as i32)
}

/// The command key at `index`, saturating at the ends of the knob's travel.
fn key_at(table: &[(&'static str, &'static str)], index: i32) -> &'static str {
    let clamped = index.clamp(0, table.len() as i32 - 1) as usize;
    table[clamped].0
}

/// Command key of the subdivision knob position `index`.
pub fn subdivision_key(index: i32) -> &'static str {
    key_at(&SUBDIVISIONS, index)
}

/// Command key of the timbre knob position `index`.
pub fn timbre_key(index: i32) -> &'static str {
    key_at(&TIMBRES, index)
}

/// Beats per bar of the time-signature knob position `index`.
pub fn time_signature_beats(index: i32) -> u32 {
    let clamped = index.clamp(0, TIME_SIGNATURES.len() as i32 - 1) as usize;
    TIME_SIGNATURES[clamped].0
}

/// Knob position for a beat count, falling back to 4/4 for a bar length the
/// knob cannot express (an MCP client is free to ask for 9 beats).
pub fn time_signature_index(beats_per_bar: u32) -> i32 {
    TIME_SIGNATURES
        .iter()
        .position(|(beats, _)| *beats == beats_per_bar)
        .map_or(DEFAULT_TIME_SIGNATURE_INDEX, |index| index as i32)
}

fn subdivision_from_key(key: &str) -> Subdivision {
    match key {
        "eighths" => Subdivision::Eighths,
        "triplets" => Subdivision::Triplets,
        "sixteenths" => Subdivision::Sixteenths,
        _ => Subdivision::Off,
    }
}

fn subdivision_to_key(subdivision: Subdivision) -> &'static str {
    match subdivision {
        Subdivision::Off => "off",
        Subdivision::Eighths => "eighths",
        Subdivision::Triplets => "triplets",
        Subdivision::Sixteenths => "sixteenths",
    }
}

fn timbre_from_key(key: &str) -> Timbre {
    match key {
        "wood" => Timbre::Wood,
        "beep" => Timbre::Beep,
        _ => Timbre::Click,
    }
}

fn timbre_to_key(timbre: Timbre) -> &'static str {
    match timbre {
        Timbre::Click => "click",
        Timbre::Wood => "wood",
        Timbre::Beep => "beep",
    }
}

/// The metronome state the window renders and `config.yaml` persists.
pub struct MetronomeSession {
    settings: MetronomeSettings,
    /// `None` follows the first available output device.
    output_device: Option<String>,
    last_tap: Option<Instant>,
    intervals: Vec<Duration>,
}

impl MetronomeSession {
    /// Restore the session from the persisted per-machine config. `enabled` is
    /// deliberately absent from `MetronomeConfig`, so a restored session is
    /// always silent until the user presses POWER.
    pub fn from_config(config: &MetronomeConfig) -> Self {
        Self {
            settings: MetronomeSettings {
                bpm: config.bpm.clamp(BPM_MIN, BPM_MAX),
                beats_per_bar: config.beats_per_bar,
                subdivision: subdivision_from_key(&config.subdivision),
                timbre: timbre_from_key(&config.timbre),
                volume: config.volume.clamp(0.0, 1.0),
                count_in: config.count_in,
            },
            output_device: config.output_device.clone(),
            last_tap: None,
            intervals: Vec::new(),
        }
    }

    pub fn settings(&self) -> MetronomeSettings {
        self.settings
    }

    pub fn bpm(&self) -> f32 {
        self.settings.bpm
    }

    pub fn set_bpm(&mut self, bpm: f32) {
        self.settings.bpm = bpm;
    }

    pub fn beats_per_bar(&self) -> u32 {
        self.settings.beats_per_bar
    }

    pub fn set_beats_per_bar(&mut self, beats_per_bar: u32) {
        self.settings.beats_per_bar = beats_per_bar;
    }

    pub fn time_signature_index(&self) -> i32 {
        time_signature_index(self.settings.beats_per_bar)
    }

    pub fn time_signature_label(&self) -> &'static str {
        let index = self.time_signature_index().max(0) as usize;
        TIME_SIGNATURES[index.min(TIME_SIGNATURES.len() - 1)].1
    }

    pub fn subdivision_key(&self) -> &'static str {
        subdivision_to_key(self.settings.subdivision)
    }

    pub fn set_subdivision_key(&mut self, key: &str) {
        self.settings.subdivision = subdivision_from_key(key);
    }

    pub fn subdivision_index(&self) -> i32 {
        index_of(&SUBDIVISIONS, self.subdivision_key()).unwrap_or(0)
    }

    pub fn subdivision_label(&self) -> &'static str {
        SUBDIVISIONS[self.subdivision_index().max(0) as usize].1
    }

    pub fn timbre_key(&self) -> &'static str {
        timbre_to_key(self.settings.timbre)
    }

    pub fn set_timbre_key(&mut self, key: &str) {
        self.settings.timbre = timbre_from_key(key);
    }

    pub fn timbre_index(&self) -> i32 {
        index_of(&TIMBRES, self.timbre_key()).unwrap_or(0)
    }

    /// Translated name of the current timbre — the only metronome label that
    /// is a word rather than a note value.
    pub fn timbre_label(&self) -> String {
        let key = TIMBRES[self.timbre_index().max(0) as usize].1;
        rust_i18n::t!(key).to_string()
    }

    pub fn volume(&self) -> f32 {
        self.settings.volume
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.settings.volume = volume;
    }

    pub fn count_in(&self) -> bool {
        self.settings.count_in
    }

    pub fn set_count_in(&mut self, count_in: bool) {
        self.settings.count_in = count_in;
    }

    pub fn output_device(&self) -> Option<&str> {
        self.output_device.as_deref()
    }

    pub fn set_output_device(&mut self, device_id: Option<String>) {
        self.output_device = device_id;
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

/// Which device the metronome's own output stream should open: the saved one
/// while it is still connected, otherwise the first device available.
///
/// Returns `None` only when the machine has no output at all — the caller then
/// has nothing to open and leaves the click off.
pub fn resolve_output_device(saved: Option<&str>, devices: &[String]) -> Option<String> {
    saved
        .filter(|id| devices.iter().any(|d| d == id))
        .map(str::to_string)
        .or_else(|| devices.first().cloned())
}

#[cfg(test)]
#[path = "metronome_session_tests.rs"]
mod tests;
