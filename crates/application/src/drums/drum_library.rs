//! Responsibility: lists the drum content found in the drum folders.
//!
//! Each drum folder holds `kits/<id>/drumkit.xml` and `grooves/<genre>.yaml`.
//! Only the kit descriptions are read here; samples load when a kit is chosen.

use std::fs;
use std::path::{Path, PathBuf};

use feature_dsp::drums::Groove;

use super::groove_file::parse_groove_file;
use super::hydrogen_kit::parse_hydrogen_kit;
use super::kit_loader::KIT_FILE;

#[derive(Clone, Debug, PartialEq)]
pub struct DrumKitEntry {
    pub id: String,
    pub name: String,
    pub dir: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct DrumLibrary {
    pub kits: Vec<DrumKitEntry>,
    pub grooves: Vec<Groove>,
}

/// The drum folder shipped with the app.
pub fn bundled_drum_dir() -> PathBuf {
    infra_filesystem::detect_data_root()
        .join("assets")
        .join("drums")
}

/// Scans `dirs` in order; an unreadable kit or groove file is logged and
/// skipped so one bad file never hides the rest.
pub fn scan_drum_library(dirs: &[PathBuf]) -> DrumLibrary {
    let mut library = DrumLibrary::default();
    for dir in dirs {
        for kit_dir in sorted_entries(&dir.join("kits")) {
            let xml_path = kit_dir.join(KIT_FILE);
            let Ok(xml) = fs::read_to_string(&xml_path) else {
                continue;
            };
            match parse_hydrogen_kit(&xml, &kit_dir) {
                Ok(kit) => library.kits.push(DrumKitEntry {
                    id: file_name(&kit_dir),
                    name: kit.name,
                    dir: kit_dir,
                }),
                Err(e) => log::warn!("skipping drum kit {xml_path:?}: {e}"),
            }
        }
        for path in sorted_entries(&dir.join("grooves")) {
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            let parsed = fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|yaml| parse_groove_file(&yaml));
            match parsed {
                Ok(grooves) => library.grooves.extend(grooves),
                Err(e) => log::warn!("skipping groove file {path:?}: {e}"),
            }
        }
    }
    library
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    paths.sort();
    paths
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
