//! Responsibility: serializes the drum machine state for a read.
//!
//! The settings come from the dispatcher; whether anything plays, and where,
//! comes from the drums' own stream when one runs.

use feature_dsp::drums::DrumPosition;

use crate::drums::DrumLibrary;
use crate::drums_state::DrumsSnapshot;

pub fn drums_state_json(
    snapshot: &DrumsSnapshot,
    library: &DrumLibrary,
    live: Option<DrumPosition>,
) -> String {
    let playing = live.map_or(snapshot.playing, |p| p.playing);
    let position = live.unwrap_or_default();
    let kits: Vec<_> = library
        .kits
        .iter()
        .map(|k| serde_json::json!({ "id": k.id, "name": k.name }))
        .collect();
    let grooves: Vec<_> = library
        .grooves
        .iter()
        .map(|g| {
            serde_json::json!({
                "id": g.id,
                "name": g.name,
                "genre": g.genre,
                "beats_per_bar": g.beats_per_bar,
                "fills": g.fills.len(),
            })
        })
        .collect();
    serde_json::json!({
        "enabled": snapshot.enabled,
        "playing": playing,
        "bpm": snapshot.bpm,
        "volume": snapshot.volume,
        "kit": snapshot.kit,
        "groove": snapshot.groove,
        "output": snapshot.output_key,
        "bar": position.bar,
        "beat": position.beat,
        "in_fill": position.in_fill,
        "kits": kits,
        "grooves": grooves,
    })
    .to_string()
}

#[cfg(test)]
#[path = "query_drums_tests.rs"]
mod tests;
