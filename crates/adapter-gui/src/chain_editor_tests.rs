//! #826 RED — the chain editor edits a chain's NAME, instrument and I/O
//! bindings. Everything else it does not edit has to come back untouched.
//!
//! It did not: the draft was rebuilt with `loopers: vec![]` and
//! `di_output: None`, so saving a rename deleted the chain's recorded loops
//! (their wavs left orphaned beside the project) and its chosen DI output.

use domain::ids::ChainId;
use project::chain::{Chain, DiOutputRef, LooperConfig};

use super::{chain_draft_from_chain, chain_from_draft};

fn recorded_chain() -> Chain {
    Chain {
        id: ChainId("rig:in".into()),
        description: Some("GUITARRA".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io-1".into()],
        blocks: vec![],
        di_output: Some(DiOutputRef {
            binding_id: "io-1".into(),
            endpoint: "out".into(),
        }),
        disabled_endpoints: Default::default(),
        loopers: vec![LooperConfig {
            audio_file: Some("rig-in-looper-1.wav".into()),
            ..LooperConfig::new(1)
        }],
    }
}

#[test]
fn editing_a_chain_keeps_what_the_editor_does_not_edit() {
    let existing = recorded_chain();
    let mut draft = chain_draft_from_chain(0, &existing);
    draft.name = "GUITARRA - TONES".into();

    let edited = chain_from_draft(&draft, Some(&existing));

    assert_eq!(
        edited.description.as_deref(),
        Some("GUITARRA - TONES"),
        "the rename is what the editor DOES edit"
    );
    assert_eq!(
        edited.loopers, existing.loopers,
        "renaming a chain must not drop the loops it recorded"
    );
    assert_eq!(
        edited.di_output, existing.di_output,
        "renaming a chain must not drop its chosen DI output"
    );
}

// ── #328: the endpoint checklists are something the editor does NOT edit ────

/// The chain editor edits name, instrument and bindings. The graph's endpoint
/// checklists are chain configuration it never shows, so a rename must hand
/// them back untouched — the #826 rule, or renaming a chain re-checks every
/// endpoint the user left out (a second guitar's input suddenly plays).
#[test]
fn editing_a_chain_keeps_its_endpoint_checklists() {
    use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};

    let mut disabled_endpoints = EndpointDisables::default();
    disabled_endpoints.set_enabled(
        EndpointNode::Input,
        EndpointRef {
            io: "io-1".into(),
            endpoint: "in 2".into(),
        },
        false,
    );
    let existing = Chain {
        disabled_endpoints,
        ..recorded_chain()
    };
    let mut draft = chain_draft_from_chain(0, &existing);
    draft.name = "GUITARRA - TONES".into();

    let edited = chain_from_draft(&draft, Some(&existing));

    assert_eq!(
        edited.disabled_endpoints, existing.disabled_endpoints,
        "renaming a chain must not re-check the endpoints its graph left out"
    );
}
