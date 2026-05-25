//! Bug repro (user, screenshot 21 May 2026 19:42): "tudo que é NAM ficou
//! [com dropdown vazio]". The block-editor model selector renders empty
//! for every chain that hits a NAM model.
//!
//! Cause: `catalog::supported_block_models(effect_type)` builds the
//! picker list with `.collect::<Result<Vec<_>, _>>()?`. If ANY single
//! registered model fails its `schema_for_block_model` lookup (stale
//! disk package manifest, missing file, etc.), the whole effect_type
//! returns `Err`. The picker caller (`block_model_picker_items`) does
//! `.unwrap_or_default()`, so an empty list ships to the GUI and the
//! dropdown shows nothing.
//!
//! Contract pin: a single bad model must NOT silence the entire list;
//! the picker must keep every model whose schema lookup succeeds and
//! skip the rest.

use project::catalog::supported_block_models;

#[test]
fn supported_block_models_returns_ok_for_every_registered_type() {
    // Contract: every well-known effect_type must return Ok, not Err.
    // A previous `?` propagation turned a single failing model into an
    // Err for the WHOLE effect_type → `block_model_picker_items` did
    // `unwrap_or_default()` → empty dropdown in the GUI.
    //
    // (List emptiness is environment-dependent — disk-package categories
    // like `body` are legitimately empty without bundled packages in the
    // test env — so only Ok-ness is asserted.)
    for effect_type in [
        block_core::EFFECT_TYPE_PREAMP,
        block_core::EFFECT_TYPE_AMP,
        block_core::EFFECT_TYPE_GAIN,
        block_core::EFFECT_TYPE_CAB,
        block_core::EFFECT_TYPE_DELAY,
        block_core::EFFECT_TYPE_REVERB,
        block_core::EFFECT_TYPE_MODULATION,
        block_core::EFFECT_TYPE_FILTER,
        block_core::EFFECT_TYPE_DYNAMICS,
        block_core::EFFECT_TYPE_WAH,
        block_core::EFFECT_TYPE_PITCH,
        block_core::EFFECT_TYPE_BODY,
    ] {
        let res = supported_block_models(effect_type);
        assert!(
            res.is_ok(),
            "effect_type '{effect_type}': supported_block_models must \
             never return Err for a known type. Got: {:?}",
            res.err()
        );
    }
}

#[test]
fn supported_block_models_gain_debug_listing() {
    // Diagnostic: dump every model the gain picker returns so we can
    // see whether NAM models are present, what type_labels they carry,
    // and whether the list is empty / partial / complete.
    let list = supported_block_models(block_core::EFFECT_TYPE_GAIN)
        .expect("gain picker must succeed");
    eprintln!("gain picker total = {}", list.len());
    for item in &list {
        eprintln!(
            "  - model_id={:?} type_label={:?} brand={:?}",
            item.model_id, item.type_label, item.brand
        );
    }
    assert!(!list.is_empty(), "gain picker list must not be empty");
}
