//! Issue #608 — the Block Editor must NOT show the catalog model
//! select/search picker for the generic NAM (`nam`) and IR (`ir`) loader
//! blocks: their "model" is a file the user loads, not a catalog entry.
//!
//! The picker visibility is derived from a pure domain predicate keyed on
//! the effect_type. nam/ir are file-loaders (no catalog) → no picker;
//! every other effect_type selects its model from the catalog → picker.

use block_core::{
    effect_type_uses_model_catalog, EFFECT_TYPE_AMP, EFFECT_TYPE_CAB, EFFECT_TYPE_GAIN,
    EFFECT_TYPE_IR, EFFECT_TYPE_NAM, EFFECT_TYPE_PREAMP, EFFECT_TYPE_REVERB,
};

#[test]
fn nam_and_ir_loaders_do_not_use_the_model_catalog() {
    assert!(
        !effect_type_uses_model_catalog(EFFECT_TYPE_NAM),
        "generic NAM loader selects a file, not a catalog model — no picker"
    );
    assert!(
        !effect_type_uses_model_catalog(EFFECT_TYPE_IR),
        "generic IR loader selects a file, not a catalog model — no picker"
    );
}

#[test]
fn catalog_backed_effect_types_use_the_model_catalog() {
    // NAM-backed pedals/amps live under their natural effect_type (gain /
    // amp / preamp) and DO pick a capture from the catalog; cab IRs (effect
    // type `cab`) likewise pick a catalog capture. All keep the picker.
    for et in [
        EFFECT_TYPE_GAIN,
        EFFECT_TYPE_AMP,
        EFFECT_TYPE_PREAMP,
        EFFECT_TYPE_CAB,
        EFFECT_TYPE_REVERB,
    ] {
        assert!(
            effect_type_uses_model_catalog(et),
            "effect_type `{et}` selects its model from the catalog — keep the picker"
        );
    }
}
