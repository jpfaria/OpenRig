//! The model a block family falls back to when the document omits it.
//!
//! A legacy or hand-written `.openrig` can leave `model:` out of a block. The
//! loader then asks the block's crate for its first supported model — so what
//! matters is that every family whose crate HAS models answers with one of
//! them. A fallback naming something unsupported would load a project the app
//! cannot build a runtime for.
//!
//! Families whose crate exposes NO model are skipped here: they have no model
//! to fall back TO, so there is nothing for these assertions to check. The
//! defect that used to make them fatal — `default_*_model()` panicking on its
//! own `expect`, taking the app down while loading a document that omitted
//! `model:` — is fixed; `default_models_tests` covers the empty family and the
//! document that carries such a block.

use super::*;

type Fallback = fn() -> String;

/// (family label, the models that family supports, its fallback)
fn families() -> Vec<(&'static str, &'static [&'static str], Fallback)> {
    vec![
        (
            "delay",
            block_delay::supported_models(),
            default_delay_model as Fallback,
        ),
        ("nam", block_nam::supported_models(), default_nam_model),
        (
            "preamp",
            block_preamp::supported_models(),
            default_preamp_model,
        ),
        ("amp", block_amp::supported_models(), default_amp_model),
        (
            "full_rig",
            block_full_rig::supported_models(),
            default_full_rig_model,
        ),
        ("cab", block_cab::supported_models(), default_cab_model),
        ("body", block_body::supported_models(), default_body_model),
        ("drive", block_gain::supported_models(), default_drive_model),
        (
            "reverb",
            block_reverb::supported_models(),
            default_reverb_model,
        ),
        (
            "utility",
            block_util::supported_models(),
            default_utility_model,
        ),
        (
            "dynamics",
            block_dyn::supported_models(),
            default_dynamics_model,
        ),
        (
            "filter",
            block_filter::supported_models(),
            default_filter_model,
        ),
        ("ir", block_ir::supported_models(), default_ir_model),
        ("wah", block_wah::supported_models(), default_wah_model),
        (
            "modulation",
            block_mod::supported_models(),
            default_modulation_model,
        ),
        (
            "pitch",
            block_pitch::supported_models(),
            default_pitch_model,
        ),
    ]
}

/// The families that can actually answer — the ones with a model to fall back to.
fn answerable() -> Vec<(&'static str, &'static [&'static str], Fallback)> {
    families()
        .into_iter()
        .filter(|(_, supported, _)| !supported.is_empty())
        .collect()
}

#[test]
fn every_family_that_has_models_falls_back_to_one_of_them() {
    let checked = answerable();
    assert!(
        checked.len() >= 10,
        "the block catalog should not have shrunk to {} families",
        checked.len()
    );
    for (family, supported, fallback) in checked {
        let model = fallback();
        assert!(
            supported.contains(&model.as_str()),
            "the {family} fallback '{model}' is not one of that crate's models: {supported:?}"
        );
    }
}

#[test]
fn the_fallback_is_the_crates_first_model_so_it_is_stable() {
    for (family, supported, fallback) in answerable() {
        assert_eq!(
            Some(fallback().as_str()),
            supported.first().copied(),
            "{family} must fall back to its crate's FIRST model — otherwise reordering \
             the catalog silently changes what a document without `model:` loads"
        );
    }
}

#[test]
fn no_answerable_family_falls_back_to_an_empty_name() {
    for (family, _, fallback) in answerable() {
        assert!(
            !fallback().is_empty(),
            "{family} has an empty fallback model"
        );
    }
}

#[test]
fn a_block_with_no_instrument_is_an_electric_guitar() {
    assert_eq!(default_instrument(), "electric_guitar");
}
