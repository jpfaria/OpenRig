//! Issue #85, second blocker: with the factory fixed (`a17a0c60`) the MCP
//! `add_block kind=output` lands the block mid-chain on the LIVE app, yet
//! clicking OUTPUT in the GUI still does nothing.
//!
//! The add flow is type → MODEL → editor. `input`/`output`/`insert` have no
//! parameter schema and no catalog model list, so if the model step has nothing
//! to offer the flow stalls there and never reaches the factory — the click
//! dies exactly as reported.
//!
//! Intended behaviour: the model list the add flow reads for an I/O port type
//! must offer its one model ("standard"), so the step can complete.

use project::catalog::supported_block_models;

#[test]
fn output_type_offers_its_standard_model() {
    let models = supported_block_models("output").unwrap_or_else(|e| {
        panic!(
            "the OUTPUT type has NO model list at all, so the add flow stalls at the model \
         step and the block is never built (#85): {e}"
        )
    });
    assert!(
        models.iter().any(|m| m.model_id == "standard"),
        "the OUTPUT type offers no model, so the add flow stalls at the model step \
         and the block is never built (#85). Models: {models:?}"
    );
}

#[test]
fn input_type_offers_its_standard_model() {
    let models = supported_block_models("input").unwrap_or_else(|e| {
        panic!(
            "the INPUT type has NO model list at all, so the add flow stalls at the model \
         step and the block is never built (#85): {e}"
        )
    });
    assert!(
        models.iter().any(|m| m.model_id == "standard"),
        "the INPUT type offers no model (#85). Models: {models:?}"
    );
}

#[test]
fn insert_type_offers_its_standard_model() {
    let models = supported_block_models("insert").unwrap_or_else(|e| {
        panic!(
            "the INSERT type has NO model list at all, so the add flow stalls at the model \
         step and the block is never built (#85): {e}"
        )
    });
    assert!(
        models.iter().any(|m| m.model_id == "standard"),
        "the INSERT type offers no model (#85). Models: {models:?}"
    );
}
