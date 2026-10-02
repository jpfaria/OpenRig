//! Responsibility: builds a playable drum kit from a kit folder.
//!
//! Runs off the audio thread: every sample is decoded, folded to mono and
//! resampled to the stream rate up front, so playback only reads memory.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use adapter_render::wav::read_wav;
use feature_dsp::drums::{DrumKit, DrumLayer, DrumPiece, DrumRole};

use super::hydrogen_kit::{parse_hydrogen_kit, KitInstrument};
use super::kit_roles::assign_roles;

/// The kit description file inside a kit folder.
pub(crate) const KIT_FILE: &str = "drumkit.xml";
/// Optional role → instrument name overrides inside a kit folder.
const ROLES_FILE: &str = "roles.yaml";

/// Loads the kit in `dir` at `sample_rate`. Any missing or unreadable sample
/// fails the whole kit, so a half-loaded kit never plays.
pub fn load_kit(dir: &Path, sample_rate: u32) -> Result<DrumKit, String> {
    let xml = fs::read_to_string(dir.join(KIT_FILE))
        .map_err(|e| format!("cannot read {:?}: {e}", dir.join(KIT_FILE)))?;
    let description = parse_hydrogen_kit(&xml, dir)?;
    let overrides = read_overrides(dir)?;
    let roles = assign_roles(&description, &overrides);

    let mut kit = DrumKit::new(&description.name, sample_rate);
    for role in DrumRole::ALL {
        if let Some(index) = roles[role.index()] {
            let piece = load_piece(&description.instruments[index], sample_rate)?;
            kit.set_piece(role, piece);
        }
    }
    Ok(kit)
}

fn read_overrides(dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let path = dir.join(ROLES_FILE);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let yaml = fs::read_to_string(&path).map_err(|e| format!("cannot read {path:?}: {e}"))?;
    serde_yaml::from_str(&yaml).map_err(|e| format!("invalid {path:?}: {e}"))
}

fn load_piece(instrument: &KitInstrument, sample_rate: u32) -> Result<DrumPiece, String> {
    let layers = instrument
        .layers
        .iter()
        .map(|layer| {
            let samples = layer
                .files
                .iter()
                .map(|file| load_sample(file, sample_rate))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(DrumLayer {
                min_velocity: layer.min_velocity,
                max_velocity: layer.max_velocity,
                samples,
                gain: layer.gain,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(DrumPiece {
        layers,
        gain: instrument.gain,
        pan: instrument.pan,
        choke_group: instrument.choke_group,
    })
}

fn load_sample(path: &Path, sample_rate: u32) -> Result<Arc<[f32]>, String> {
    let wav = read_wav(path).map_err(|e| format!("cannot read {path:?}: {e}"))?;
    let channels = usize::from(wav.channels.max(1));
    let mono: Vec<f32> = wav
        .samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    let path_name = path.to_string_lossy();
    Ok(ir::resample_if_needed(mono, wav.sample_rate_hz, sample_rate as f32, &path_name).into())
}
