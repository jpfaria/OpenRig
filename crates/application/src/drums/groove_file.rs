//! Responsibility: parses a genre groove file into grooves.
//!
//! A file holds one genre; each hit is `[beat, role, velocity 0-127]`.

use feature_dsp::drums::{DrumHit, DrumPattern, DrumRole, Groove};
use serde::Deserialize;

#[derive(Deserialize)]
struct GrooveFile {
    genre: String,
    grooves: Vec<GrooveEntry>,
}

#[derive(Deserialize)]
struct GrooveEntry {
    id: String,
    name: String,
    beats_per_bar: u32,
    tempo: f32,
    beat: PatternEntry,
    #[serde(default)]
    fills: Vec<PatternEntry>,
}

#[derive(Deserialize)]
struct PatternEntry {
    beats: f64,
    hits: Vec<(f64, String, f32)>,
}

pub fn parse_groove_file(yaml: &str) -> Result<Vec<Groove>, String> {
    let file: GrooveFile =
        serde_yaml::from_str(yaml).map_err(|e| format!("invalid groove file: {e}"))?;
    file.grooves
        .into_iter()
        .map(|entry| {
            Ok(Groove {
                beat: pattern(&entry.beat, &entry.id)?,
                fills: entry
                    .fills
                    .iter()
                    .map(|fill| pattern(fill, &entry.id))
                    .collect::<Result<_, String>>()?,
                id: entry.id,
                name: entry.name,
                genre: file.genre.clone(),
                beats_per_bar: entry.beats_per_bar,
                tempo: entry.tempo,
            })
        })
        .collect()
}

fn pattern(entry: &PatternEntry, groove_id: &str) -> Result<DrumPattern, String> {
    let hits = entry
        .hits
        .iter()
        .map(|(beat, role, velocity)| {
            let role = DrumRole::from_key(role)
                .ok_or_else(|| format!("groove '{groove_id}': unknown role '{role}'"))?;
            Ok(DrumHit {
                beat: *beat,
                role,
                velocity: (velocity / 127.0).clamp(0.0, 1.0),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(DrumPattern::new(entry.beats, hits))
}
