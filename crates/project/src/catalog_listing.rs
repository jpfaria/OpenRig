//! Responsibility: lists what block types the app can offer.

use crate::block::schema_for_block_model;
use crate::catalog_label::package_type_label;
use crate::catalog_registry::block_registry;
use crate::catalog_types::{BlockModelCatalogEntry, BlockTypeCatalogEntry};

pub fn supported_block_types() -> Vec<BlockTypeCatalogEntry> {
    let mut types: Vec<_> = block_registry()
        .into_iter()
        .filter(|entry| {
            // Include the type if it has either native models OR
            // disk-backed packages registered for it. Block types that
            // migrated entirely to disk packages (e.g. block-body) have
            // an empty native slice but still need to appear in the GUI.
            // Issue #287.
            if !(entry.supported_models)().is_empty() {
                return true;
            }
            block_type_for_effect_type(entry.effect_type)
                .map(|bt| !plugin_loader::registry::packages_for(bt).is_empty())
                .unwrap_or(false)
        })
        .map(|entry| BlockTypeCatalogEntry {
            effect_type: entry.effect_type,
            display_label: entry.display_label,
            icon_kind: entry.icon_kind,
            use_panel_editor: entry.use_panel_editor,
        })
        .collect();
    // Include the VST3 dynamic type only if plugins have been discovered.
    if !vst3_host::vst3_catalog().is_empty() {
        types.push(BlockTypeCatalogEntry {
            effect_type: block_core::EFFECT_TYPE_VST3,
            display_label: "VST3",
            icon_kind: block_core::EFFECT_TYPE_VST3,
            use_panel_editor: true,
        });
    }
    log::trace!("supported_block_types: {} types registered", types.len());
    types
}

pub fn supported_block_type(effect_type: &str) -> Option<BlockTypeCatalogEntry> {
    if effect_type == block_core::EFFECT_TYPE_VST3 {
        return Some(BlockTypeCatalogEntry {
            effect_type: block_core::EFFECT_TYPE_VST3,
            display_label: "VST3",
            icon_kind: block_core::EFFECT_TYPE_VST3,
            use_panel_editor: true,
        });
    }
    block_registry()
        .into_iter()
        .find(|entry| entry.effect_type == effect_type)
        .map(|entry| BlockTypeCatalogEntry {
            effect_type: entry.effect_type,
            display_label: entry.display_label,
            icon_kind: entry.icon_kind,
            use_panel_editor: entry.use_panel_editor,
        })
}

/// The single catalog entry of an I/O port type, or `None` for a normal
/// (registry-backed) effect. An I/O port has one model and no knobs — what the
/// user picks afterwards is its binding endpoint, not a plugin.
fn io_port_model(effect_type: &str) -> Option<BlockModelCatalogEntry> {
    let display_name = match effect_type {
        block_core::constants::EFFECT_TYPE_INPUT => "Input",
        block_core::constants::EFFECT_TYPE_OUTPUT => "Output",
        block_core::constants::EFFECT_TYPE_INSERT => "Insert",
        _ => return None,
    };
    Some(BlockModelCatalogEntry {
        effect_type: effect_type.to_string(),
        model_id: block_core::constants::IO_PORT_MODEL.to_string(),
        display_name: display_name.to_string(),
        brand: block_core::BRAND_NATIVE.to_string(),
        type_label: "I/O".to_string(),
        supported_instruments: block_core::ALL_INSTRUMENTS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        knob_layout: &[],
    })
}

pub fn supported_block_models(effect_type: &str) -> Result<Vec<BlockModelCatalogEntry>, String> {
    log::trace!("looking up models for effect_type='{}'", effect_type);

    // Dynamic VST3 catalog — bypass the static block_registry.
    if effect_type == block_core::EFFECT_TYPE_VST3 {
        return Ok(vst3_host::vst3_catalog()
            .iter()
            .map(|entry| BlockModelCatalogEntry {
                effect_type: block_core::EFFECT_TYPE_VST3.to_string(),
                model_id: entry.model_id.to_string(),
                display_name: entry.display_name.to_string(),
                brand: entry.brand.to_string(),
                type_label: "VST3".to_string(),
                supported_instruments: block_core::ALL_INSTRUMENTS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                knob_layout: &[],
            })
            .collect());
    }

    // I/O port types (#85): `input` / `output` / `insert` are not in the block
    // registry — they have no parameter schema, they reference a binding
    // endpoint. They must still answer the model step of the add flow with
    // their single model, or the GUI stalls right after the type is picked and
    // the click does nothing at all.
    if let Some(entry) = io_port_model(effect_type) {
        return Ok(vec![entry]);
    }

    let disk_pkg_instruments: Vec<String> = default_instruments_for_effect_type(effect_type)
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    let entry = block_registry()
        .into_iter()
        .find(|entry| entry.effect_type == effect_type)
        .ok_or_else(|| format!("unsupported effect type '{}'", effect_type))?;

    // Build the per-model catalog entry, skipping (and logging) any
    // model whose schema lookup fails. Pre-fix this was a `?`
    // propagation that turned a single bad model into an Err for the
    // whole effect_type — `block_model_picker_items` then did
    // `unwrap_or_default()` and the GUI dropdown went **completely
    // empty** for every chain using that effect_type (e.g. "every NAM"
    // — user report 21 May 2026). One bad disk-package manifest must
    // not silence the entire list.
    let mut result: Vec<BlockModelCatalogEntry> = (entry.supported_models)()
        .iter()
        .filter_map(|model_id| {
            let schema = match schema_for_block_model(effect_type, model_id) {
                Ok(s) => s,
                Err(err) => {
                    log::warn!(
                        "[catalog] skipping model '{}' (effect_type='{}'): {}",
                        model_id,
                        effect_type,
                        err
                    );
                    return None;
                }
            };
            let visual = (entry.model_visual)(model_id);
            Some(Ok::<_, String>(BlockModelCatalogEntry {
                effect_type: effect_type.to_string(),
                model_id: (*model_id).to_string(),
                display_name: schema.display_name,
                brand: visual
                    .as_ref()
                    .map(|v| v.brand.to_string())
                    .unwrap_or_default(),
                type_label: visual
                    .as_ref()
                    .map(|v| v.type_label.to_string())
                    .unwrap_or_default(),
                supported_instruments: visual
                    .as_ref()
                    .map(|v| {
                        v.supported_instruments
                            .iter()
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_else(|| {
                        block_core::ALL_INSTRUMENTS
                            .iter()
                            .map(|s| s.to_string())
                            .collect()
                    }),
                knob_layout: visual.as_ref().map(|v| v.knob_layout).unwrap_or(&[]),
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Merge in disk-backed packages whose `block_type` matches this
    // `effect_type`. Native models still pass through the static
    // `entry.supported_models` slice above; disk packages were absent
    // from that slice and so wouldn't surface to the GUI before this
    // change. Issue #287.
    if let Some(block_type) = block_type_for_effect_type(effect_type) {
        let already: std::collections::HashSet<String> =
            result.iter().map(|e| e.model_id.clone()).collect();
        for package in plugin_loader::registry::packages_for(block_type) {
            if already.contains(&package.manifest.id) {
                continue;
            }
            let visual = (entry.model_visual)(package.manifest.id.as_str());
            let type_label = visual
                .as_ref()
                .map(|v| v.type_label.to_string())
                .unwrap_or_else(|| package_type_label(&package.manifest));
            result.push(BlockModelCatalogEntry {
                effect_type: effect_type.to_string(),
                model_id: package.manifest.id.clone(),
                display_name: package.manifest.display_name.clone(),
                brand: package.manifest.brand.clone().unwrap_or_default(),
                type_label,
                supported_instruments: disk_pkg_instruments.clone(),
                knob_layout: &[],
            });
        }
    }
    Ok(result)
}

/// Map a stable `effect_type` string to the discriminant the
/// plugin-loader registry uses. Returns `None` for `effect_type` values
/// that don't correspond to a [`plugin_loader::manifest::BlockType`]
/// variant — those don't have disk-package support.
pub(crate) fn block_type_for_effect_type(
    effect_type: &str,
) -> Option<plugin_loader::manifest::BlockType> {
    use block_core::*;
    use plugin_loader::manifest::BlockType;
    Some(match effect_type {
        s if s == EFFECT_TYPE_PREAMP => BlockType::Preamp,
        s if s == EFFECT_TYPE_AMP => BlockType::Amp,
        s if s == EFFECT_TYPE_CAB => BlockType::Cab,
        s if s == EFFECT_TYPE_BODY => BlockType::Body,
        s if s == EFFECT_TYPE_GAIN => BlockType::GainPedal,
        s if s == EFFECT_TYPE_DELAY => BlockType::Delay,
        s if s == EFFECT_TYPE_REVERB => BlockType::Reverb,
        s if s == EFFECT_TYPE_MODULATION => BlockType::Mod,
        s if s == EFFECT_TYPE_DYNAMICS => BlockType::Dyn,
        s if s == EFFECT_TYPE_FILTER => BlockType::Filter,
        s if s == EFFECT_TYPE_WAH => BlockType::Wah,
        s if s == EFFECT_TYPE_PITCH => BlockType::Pitch,
        s if s == EFFECT_TYPE_UTILITY => BlockType::Util,
        _ => return None,
    })
}

/// Default instrument list for disk-package models keyed by effect_type.
///
/// Issue #403: previously every disk package declared `ALL_INSTRUMENTS`, so the
/// "add block" picker on a `voice` chain still showed Amp/Cab/Wah/etc — those
/// categories don't apply to vocals. Native models carry per-model
/// `visual.supported_instruments`; disk packages don't, so we infer from the
/// category. Categories with potential cross-instrument use (Dyn, Filter, Mod,
/// Reverb, Delay, Pitch, Util) stay universal; guitar/bass-only categories
/// (Amp, Cab, GainPedal, Wah) drop voice/keys/drums.
fn default_instruments_for_effect_type(effect_type: &str) -> &'static [&'static str] {
    use block_core::*;
    match effect_type {
        EFFECT_TYPE_AMP | EFFECT_TYPE_CAB | EFFECT_TYPE_GAIN | EFFECT_TYPE_WAH => GUITAR_BASS,
        EFFECT_TYPE_PREAMP => GUITAR_ACOUSTIC_BASS,
        EFFECT_TYPE_BODY => &[INST_ACOUSTIC_GUITAR, INST_BASS],
        _ => ALL_INSTRUMENTS,
    }
}
