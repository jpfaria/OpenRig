//! Issue #403 — block-type picker on a `voice` chain must hide guitar/bass-only
//! categories (Amp, Cab, GainPedal, Wah). Disk-package models previously
//! declared `ALL_INSTRUMENTS`, so every category surfaced regardless of the
//! chain's instrument. After the catalog fix, disk-packages inherit a
//! category-derived default and the picker filter (in adapter-gui) excludes
//! categories with no voice-supporting models.

use block_core::{INST_ELECTRIC_GUITAR, INST_VOICE};
use project::catalog::supported_block_models;

/// Initialize the plugin registry once for all tests in this file. The catalog
/// merges native models with disk-packages discovered via the registry, so
/// without this the disk-package contribution (LV2 pitch plugins, etc.) is
/// invisible.
fn init_plugins() {
    use std::path::PathBuf;
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../../OpenRig-plugins/plugins/source"),
            PathBuf::from(
                "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
            ),
        ];
        let roots: Vec<PathBuf> = candidates.into_iter().filter(|p| p.is_dir()).collect();
        if !roots.is_empty() {
            plugin_loader::registry::init_many(&roots);
        }
    });
}

/// On a `voice` chain, no disk-package amp/cab/wah model should claim voice
/// support. (Native models are unaffected — they carry per-model
/// `visual.supported_instruments` and `gain` natives like `volume` and
/// `tape_saturation` are intentionally voice-friendly.)
#[test]
fn voice_chain_excludes_disk_amp_cab_wah() {
    init_plugins();
    for effect_type in ["amp", "cab", "wah"] {
        let models = supported_block_models(effect_type).expect("catalog lookup");
        let disk_voice: Vec<&str> = models
            .iter()
            .filter(|m| {
                // Only consider disk-packages (non-native brands/IDs aren't
                // a perfect signal; instead check via plugin_loader::registry).
                let is_disk = !m.brand.is_empty() && m.brand != "native";
                is_disk && m.supported_instruments.iter().any(|i| i == INST_VOICE)
            })
            .map(|m| m.model_id.as_str())
            .collect();
        assert!(
            disk_voice.is_empty(),
            "disk-package {} models should not claim voice support: {:?}",
            effect_type,
            disk_voice,
        );
    }
}

/// Sanity: voice-relevant categories (filter, dynamics, modulation, reverb,
/// delay, pitch) must still produce models for voice chains — otherwise the
/// picker hides every category and the user can't add anything.
#[test]
fn voice_chain_includes_voice_relevant_categories() {
    init_plugins();
    for effect_type in [
        "filter",
        "dynamics",
        "modulation",
        "reverb",
        "delay",
        "pitch",
    ] {
        let models = supported_block_models(effect_type).expect("catalog lookup");
        let voice_count = models
            .iter()
            .filter(|m| m.supported_instruments.iter().any(|i| i == INST_VOICE))
            .count();
        assert!(
            voice_count > 0,
            "{}: expected at least one voice-supporting model, got 0",
            effect_type,
        );
    }
}

/// Guitar still sees amp/cab/gain/wah models — the fix tightens voice, not guitar.
#[test]
fn guitar_chain_still_sees_guitar_categories() {
    init_plugins();
    for effect_type in ["amp", "cab", "gain", "wah"] {
        let models = supported_block_models(effect_type).expect("catalog lookup");
        let guitar_count = models
            .iter()
            .filter(|m| {
                m.supported_instruments
                    .iter()
                    .any(|i| i == INST_ELECTRIC_GUITAR)
            })
            .count();
        assert!(
            guitar_count > 0,
            "{}: expected models for electric_guitar, got 0",
            effect_type,
        );
    }
}
