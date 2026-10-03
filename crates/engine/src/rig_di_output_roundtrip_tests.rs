//! #324: the DI output picked on a projected chain is captured into the rig
//! and comes back on the next projection.

use std::collections::BTreeSet;

use project::endpoint_ref::DiOutputRef;
use project::rig_sync::sync_synthetic_into_rig;

use super::instrument_roundtrip_tests::simple_rig;
use crate::rig_runtime::rig_to_legacy_project;

#[test]
fn di_output_survives_sync_and_reprojection() {
    let mut rig = simple_rig();
    let mut proj = rig_to_legacy_project(&rig, &BTreeSet::new());
    let di_output = Some(DiOutputRef {
        binding_id: "monitors".into(),
        endpoint: "FRFR".into(),
    });
    proj.chains
        .iter_mut()
        .find(|c| c.id.0 == "rig:input-1")
        .unwrap()
        .di_output = di_output.clone();

    sync_synthetic_into_rig(&mut rig, &proj);
    let reprojected = rig_to_legacy_project(&rig, &BTreeSet::new());

    let chain = reprojected
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:input-1")
        .unwrap();
    assert_eq!(chain.di_output, di_output);
}
