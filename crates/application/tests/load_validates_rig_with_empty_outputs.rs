//! End-to-end pin for the CABELINHO screenshot ("INVALID PROJECT: chain
//! 'rig:input-4' has no output blocks"):
//!
//!   rig with empty `rig.outputs`
//! → `rig_to_legacy_project` produces a chain with NO Output
//! → `ensure_chains_have_output` synthesises a default Output
//! → `validate_project` must pass.
//!
//! If this test ever flips RED again, the migration safety net broke and
//! every old project reopens unusable.

use std::collections::BTreeMap;

use domain::ids::{ChainId, DeviceId};
use project::block::{AudioBlockKind, InputEntry};
use project::chain::ChainInputMode;
use project::rig::{RigInput, RigPreset, RigProject};

use application::validate::validate_project;

const INPUT_NAME: &str = "input-4";
const CHAIN_ID: &str = "rig:input-4";
const DEVICE: &str = "coreaudio:default";

fn rig_without_outputs() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        INPUT_NAME.into(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: DeviceId(DEVICE.into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
        },
    );
    RigProject {
        name: None,
        inputs,
        presets,
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        midi: None,
    }
}

#[test]
fn rig_to_legacy_project_chain_has_no_output_without_routing() {
    // Documents the baseline: rig-projected chain is born without an
    // Output when rig.outputs is empty.
    let rig = rig_without_outputs();
    let project =
        engine::rig_runtime::rig_to_legacy_project(&rig, &std::collections::BTreeSet::new());
    let chain = project
        .chains
        .iter()
        .find(|c| c.id.0 == CHAIN_ID)
        .expect("rig projected the chain");
    let outputs = chain
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
        .count();
    assert_eq!(outputs, 0);
}

#[test]
fn ensure_chains_have_output_makes_validate_project_pass() {
    // The end-to-end claim of the migration fix: after
    // `ensure_chains_have_output`, the project is validate-clean and the
    // runtime can start without further intervention.
    let rig = rig_without_outputs();
    let mut project = engine::rig_runtime::rig_to_legacy_project(
        &rig,
        // The chain MUST be enabled to exercise the validator's
        // output-blocks branch -- the validator only checks enabled
        // chains.
        &std::iter::once(INPUT_NAME.to_string()).collect(),
    );
    // Before the migration: validate rejects.
    let before = validate_project(&project);
    assert!(
        before.is_err(),
        "precondition: enabled chain without Output must fail validate \
         today; got Ok unexpectedly"
    );

    project::project_ensure_io::ensure_chains_have_output(&mut project, &DeviceId(DEVICE.into()));

    let after = validate_project(&project);
    assert!(
        after.is_ok(),
        "after ensure_chains_have_output, validate must pass. Got: {:?}",
        after.err()
    );

    // Sanity-check: the chain id used in the live screenshot.
    assert!(
        project
            .chains
            .iter()
            .any(|c| c.id == ChainId(CHAIN_ID.into())),
        "the synthesized output belongs to rig:input-4 (CABELINHO)"
    );
}
