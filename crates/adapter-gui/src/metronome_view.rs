//! Responsibility: builds what the metronome window shows.
//! What the metronome window SHOWS (#14) — the knob vocabulary and the output
//! endpoints the click can play through. No state lives here.
//!
//! #127: this module used to own a `MetronomeSession` — the settings, the
//! chosen output and the tap history — which is why a footswitch or an MCP
//! client could flip `metronome_enabled` and hear nothing: only the GUI held
//! the settings, and only its own knob callbacks knew how to turn an event
//! into sound. That state now belongs to the dispatcher
//! (`application::metronome_state`), and this module renders whatever snapshot
//! it hands over.
//!
//! What remains is genuinely the window's: the index↔key↔label translation for
//! the three knobs (the knobs speak indices, the `Command`s speak strings like
//! `"eighths"`), and the project's output endpoints. Every conversion lives
//! here so no call site has to remember the order of a list.
//!
//! The beat lamps are NOT driven from here: the wiring's timer reads the click's
//! position through `LiveSource` — a phase, not a queue of events — so a slow
//! frame can never lose or double a beat.

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

/// Label of the time signature the knob points at for `beats_per_bar`.
pub fn time_signature_label(beats_per_bar: u32) -> &'static str {
    let index = time_signature_index(beats_per_bar).max(0) as usize;
    TIME_SIGNATURES[index.min(TIME_SIGNATURES.len() - 1)].1
}

/// Knob position for a subdivision command key. An unknown key rests on the
/// first position rather than pointing at a random one.
pub fn subdivision_index(key: &str) -> i32 {
    index_of(&SUBDIVISIONS, key).unwrap_or(0)
}

/// Note-value label of a subdivision command key.
pub fn subdivision_label(key: &str) -> &'static str {
    SUBDIVISIONS[subdivision_index(key).max(0) as usize].1
}

/// Knob position for a timbre command key.
pub fn timbre_index(key: &str) -> i32 {
    index_of(&TIMBRES, key).unwrap_or(0)
}

/// Translated name of a timbre — the only metronome label that is a word
/// rather than a note value.
pub fn timbre_label(key: &str) -> String {
    let translation_key = TIMBRES[timbre_index(key).max(0) as usize].1;
    rust_i18n::t!(translation_key).to_string()
}

/// One selectable metronome output: an output endpoint of one of the project's
/// I/O bindings (#14). The metronome plays through the SAME outputs the project
/// is configured with, not a raw device list — so it lands on the channels the
/// user already set up.
#[derive(Debug, Clone, PartialEq)]
pub struct MetronomeOutput {
    /// Stable key `"{binding_id}\u{1f}{endpoint_name}"`, round-tripped by the
    /// select and persisted in `config.yaml`.
    pub key: String,
    /// `"{binding name} · {endpoint name}"`, shown in the picker.
    pub label: String,
    pub device_id: String,
    pub channels: Vec<usize>,
}

/// The key that identifies an output endpoint. The unit separator keeps it
/// unambiguous even if a binding id or endpoint name contains a space or dot.
pub fn endpoint_key(binding_id: &str, endpoint_name: &str) -> String {
    format!("{binding_id}\u{1f}{endpoint_name}")
}

/// Every output endpoint the project's bindings expose, in registry order.
pub fn output_endpoints(bindings: &[infra_filesystem::IoBinding]) -> Vec<MetronomeOutput> {
    bindings
        .iter()
        .flat_map(|binding| {
            binding.outputs.iter().map(move |endpoint| MetronomeOutput {
                key: endpoint_key(&binding.id, &endpoint.name),
                label: format!("{} · {}", binding.name, endpoint.name),
                device_id: endpoint.device_id.0.clone(),
                channels: endpoint.channels.clone(),
            })
        })
        .collect()
}

/// Resolve the saved endpoint key to a concrete output: the saved one while it
/// still exists, otherwise the first endpoint (a renamed binding or a different
/// machine must not leave the metronome silent). `None` only when the project
/// has no output endpoint at all.
pub fn resolve_output_endpoint(
    saved: Option<&str>,
    endpoints: &[MetronomeOutput],
) -> Option<MetronomeOutput> {
    saved
        .and_then(|key| endpoints.iter().find(|o| o.key == key).cloned())
        .or_else(|| endpoints.first().cloned())
}

#[cfg(test)]
#[path = "metronome_view_tests.rs"]
mod tests;
