//! Responsibility: builds what the drum machine panel shows.

use application::drums::DrumLibrary;
use application::drums_state::DrumsSnapshot;

use feature_dsp::drums::{MAX_BPM, MIN_BPM};

use crate::metronome_view::{resolve_output_endpoint, ProjectOutput};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DrumPick {
    pub key: String,
    pub label: String,
    pub header: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct DrumsView {
    pub enabled: bool,
    pub playing: bool,
    pub bpm: f32,
    pub volume: f32,
    pub bpm_min: f32,
    pub bpm_max: f32,
    pub kits: Vec<DrumPick>,
    pub grooves: Vec<DrumPick>,
    pub outputs: Vec<DrumPick>,
    pub kit_key: String,
    pub kit_label: String,
    pub groove_key: String,
    pub groove_label: String,
    pub output_key: String,
    pub output_label: String,
    pub beats_per_bar: i32,
}

/// Everything the panel draws, from the dispatcher's state. Genre headers
/// group the grooves in the order each genre first appears in the library.
pub(crate) fn drums_view(
    snapshot: &DrumsSnapshot,
    library: &DrumLibrary,
    endpoints: &[ProjectOutput],
) -> DrumsView {
    let kit = library
        .kits
        .iter()
        .find(|k| Some(k.id.as_str()) == snapshot.kit.as_deref());
    let groove = library
        .grooves
        .iter()
        .find(|g| Some(g.id.as_str()) == snapshot.groove.as_deref());
    let output = resolve_output_endpoint(snapshot.output_key.as_deref(), endpoints);
    DrumsView {
        enabled: snapshot.enabled,
        playing: snapshot.playing,
        bpm: snapshot.bpm,
        volume: snapshot.volume,
        bpm_min: MIN_BPM,
        bpm_max: MAX_BPM,
        kits: library
            .kits
            .iter()
            .map(|k| choice(&k.id, &k.name))
            .collect(),
        grooves: groove_rows(library),
        outputs: endpoints.iter().map(|o| choice(&o.key, &o.label)).collect(),
        kit_key: kit.map(|k| k.id.clone()).unwrap_or_default(),
        kit_label: kit.map(|k| k.name.clone()).unwrap_or_default(),
        groove_key: groove.map(|g| g.id.clone()).unwrap_or_default(),
        groove_label: groove.map(|g| g.name.clone()).unwrap_or_default(),
        output_key: output.as_ref().map(|o| o.key.clone()).unwrap_or_default(),
        output_label: output.map(|o| o.label).unwrap_or_default(),
        beats_per_bar: groove.map_or(DEFAULT_BEATS_PER_BAR, |g| g.beats_per_bar as i32),
    }
}

/// The bar a panel shows before any groove is chosen.
const DEFAULT_BEATS_PER_BAR: i32 = 4;

fn choice(key: &str, label: &str) -> DrumPick {
    DrumPick {
        key: key.to_string(),
        label: label.to_string(),
        header: false,
    }
}

fn groove_rows(library: &DrumLibrary) -> Vec<DrumPick> {
    let mut genres: Vec<&str> = Vec::new();
    for groove in &library.grooves {
        if !genres.contains(&groove.genre.as_str()) {
            genres.push(&groove.genre);
        }
    }
    genres
        .into_iter()
        .flat_map(|genre| {
            let header = DrumPick {
                key: String::new(),
                label: genre.to_string(),
                header: true,
            };
            let members = library
                .grooves
                .iter()
                .filter(move |g| g.genre == genre)
                .map(|g| choice(&g.id, &g.name));
            std::iter::once(header).chain(members)
        })
        .collect()
}

#[cfg(test)]
#[path = "drums_view_tests.rs"]
mod tests;
