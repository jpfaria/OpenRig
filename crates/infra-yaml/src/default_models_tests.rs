//! #913 — the model a block family falls back to when the document omits it.
//!
//! Each default is the FIRST entry of its crate's `supported_models()`, so a
//! document that names a family but no model still deserializes into something
//! the registry can build. What must hold: the answer is never empty, and it is
//! a model the family actually supports — a stale literal here would load a
//! project into a model the build path then rejects.

use super::*;

fn assert_is_first_supported(actual: String, supported: &[&str]) {
    assert_eq!(
        actual,
        supported.first().copied().unwrap_or_default(),
        "the default is the family's first supported model"
    );
}

#[test]
fn every_block_family_defaults_to_its_first_supported_model() {
    assert_is_first_supported(default_delay_model(), &block_delay::supported_models());
    assert_is_first_supported(default_nam_model(), &block_nam::supported_models());
    assert_is_first_supported(default_preamp_model(), &block_preamp::supported_models());
    assert_is_first_supported(default_amp_model(), &block_amp::supported_models());
    assert_is_first_supported(
        default_full_rig_model(),
        &block_full_rig::supported_models(),
    );
    assert_is_first_supported(default_cab_model(), &block_cab::supported_models());
    assert_is_first_supported(default_body_model(), &block_body::supported_models());
    assert_is_first_supported(default_drive_model(), &block_gain::supported_models());
    assert_is_first_supported(default_reverb_model(), &block_reverb::supported_models());
    assert_is_first_supported(default_utility_model(), &block_util::supported_models());
    assert_is_first_supported(default_dynamics_model(), &block_dyn::supported_models());
    assert_is_first_supported(default_filter_model(), &block_filter::supported_models());
    assert_is_first_supported(default_ir_model(), &block_ir::supported_models());
    assert_is_first_supported(default_wah_model(), &block_wah::supported_models());
    assert_is_first_supported(default_modulation_model(), &block_mod::supported_models());
    assert_is_first_supported(default_pitch_model(), &block_pitch::supported_models());
}

#[test]
fn a_family_with_no_registered_model_defaults_to_an_empty_one_instead_of_panicking() {
    // `block-full-rig` ships zero models. Before #913 the default asserted the
    // family was non-empty, so a document carrying `type: full_rig` with no
    // `model:` aborted the load instead of being reported as an unsupported
    // model.
    assert!(block_full_rig::supported_models().is_empty());
    assert_eq!(default_full_rig_model(), "");
}

#[test]
fn a_full_rig_block_without_a_model_loads_with_the_block_dropped() {
    let yaml = r#"
chains:
  - description: rig without a model
    blocks:
      - type: full_rig
        enabled: true
"#;
    let dto: crate::ProjectYaml = serde_yaml::from_str(yaml).expect("the document must parse");
    let project = dto
        .into_project()
        .expect("the rest of the document survives one unbuildable block");
    assert_eq!(project.chains.len(), 1);
    assert!(
        project.chains[0].blocks.is_empty(),
        "the empty model resolves to no definition, so the block is dropped — not a panic"
    );
}

#[test]
fn a_block_is_enabled_unless_the_document_says_otherwise() {
    assert!(default_enabled());
}

#[test]
fn an_input_defaults_to_the_shared_instrument_constant() {
    assert_eq!(default_instrument(), block_core::DEFAULT_INSTRUMENT);
    assert!(!default_instrument().is_empty());
}
