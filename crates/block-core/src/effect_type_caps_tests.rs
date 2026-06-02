use super::*;
use crate::constants::{
    EFFECT_TYPE_AMP, EFFECT_TYPE_CAB, EFFECT_TYPE_DELAY, EFFECT_TYPE_DYNAMICS, EFFECT_TYPE_FILTER,
    EFFECT_TYPE_GAIN, EFFECT_TYPE_MODULATION, EFFECT_TYPE_PREAMP, EFFECT_TYPE_REVERB,
};

#[test]
fn file_loader_effect_types_do_not_use_catalog() {
    assert!(!effect_type_uses_model_catalog(EFFECT_TYPE_NAM));
    assert!(!effect_type_uses_model_catalog(EFFECT_TYPE_IR));
}

#[test]
fn every_other_effect_type_uses_catalog() {
    for et in [
        EFFECT_TYPE_GAIN,
        EFFECT_TYPE_AMP,
        EFFECT_TYPE_PREAMP,
        EFFECT_TYPE_CAB,
        EFFECT_TYPE_REVERB,
        EFFECT_TYPE_DELAY,
        EFFECT_TYPE_DYNAMICS,
        EFFECT_TYPE_FILTER,
        EFFECT_TYPE_MODULATION,
    ] {
        assert!(
            effect_type_uses_model_catalog(et),
            "effect_type `{et}` should keep the catalog picker"
        );
    }
}

#[test]
fn unknown_effect_type_defaults_to_catalog() {
    // A not-yet-known effect_type keeps the picker (only nam/ir are file
    // loaders); fail-open is the safe default for an unrecognised type.
    assert!(effect_type_uses_model_catalog("totally_new_type"));
}
