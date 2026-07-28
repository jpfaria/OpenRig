//! #323 — the looper rows a chain card shows on project open.
//!
//! Bug (user-reported): after saving a project with a looper, reopening it
//! showed NO loopers in the panel, and clicking "Add" kept accumulating
//! loopers in the config until "chain already holds the maximum of 8" —
//! because `replace_project_chains` left the row's `loopers` empty and only
//! the meter timer ever filled it, which never visits a chain with no live
//! stream. The rows must come from the chain's persisted `LooperConfig`.

use crate::project_view::replace_project_chains;
use crate::ProjectChainItem;
use domain::ids::ChainId;
use project::chain::{Chain, LooperConfig, LooperSpeed};
use project::project::Project;
use slint::{Model, VecModel};
use std::rc::Rc;

fn chain_with_loopers(enabled: bool, loopers: Vec<LooperConfig>) -> Chain {
    Chain {
        id: ChainId("test:chain".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers,
    }
}

fn rows(project: &Project) -> Vec<crate::LooperItem> {
    let model = Rc::new(VecModel::<ProjectChainItem>::default());
    replace_project_chains(&model, project, &[], &[], &[]);
    model
        .row_data(0)
        .expect("one chain row")
        .loopers
        .iter()
        .collect()
}

#[test]
fn a_reopened_chains_persisted_loopers_become_rows() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain_with_loopers(
            false,
            vec![LooperConfig::new(1), LooperConfig::new(2)],
        )],
        midi: None,
    };
    let rows = rows(&project);
    assert_eq!(
        rows.len(),
        2,
        "both persisted loopers must show on open, even with the chain disabled"
    );
    assert_eq!(rows[0].uid, 1);
    assert_eq!(rows[1].uid, 2);
    // No live runtime yet, so every looper reads as empty.
    assert_eq!(rows[0].state_code, 0);
    assert_eq!(rows[0].layers, 0);
}

#[test]
fn a_chain_with_no_loopers_shows_no_rows() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain_with_loopers(true, vec![])],
        midi: None,
    };
    assert!(rows(&project).is_empty());
}

#[test]
fn persisted_looper_parameters_reach_the_row() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain_with_loopers(
            false,
            vec![LooperConfig {
                uid: 5,
                mix: 0.5,
                decay: 0.25,
                speed: LooperSpeed::Double,
                reverse: true,
                audio_file: Some("loop.wav".into()),
                input: None,
                output: None,
                preset: None,
            }],
        )],
        midi: None,
    };
    let rows = rows(&project);
    assert_eq!(rows[0].mix, 50);
    assert_eq!(rows[0].decay, 25);
    assert_eq!(rows[0].speed_index, 2);
    assert!(rows[0].reverse);
}

// ── live refresh without a running stream (the accumulate-to-8 path) ─────

/// Adding a looper to a chain that has NO live stream must still show up: the
/// callback rebuilds the row from the persisted config via
/// `write_chain_looper_row(.., None)`. Before this, only the meter timer
/// filled the rows and it never visits a stream-less chain, so the panel
/// stayed empty and Add silently piled loopers into the config.
#[test]
fn write_row_without_a_runtime_reflects_the_config() {
    let mut chain = chain_with_loopers(true, vec![]);
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain.clone()],
        midi: None,
    };
    let model = Rc::new(VecModel::<ProjectChainItem>::default());
    replace_project_chains(&model, &project, &[], &[], &[]);
    assert_eq!(model.row_data(0).unwrap().loopers.row_count(), 0);

    // A looper was just added to the config (no stream, so `sample_rate` None).
    chain.loopers.push(LooperConfig::new(1));
    crate::looper_view::write_chain_looper_row(&model, 0, &chain, &[], None, &[], &[]);

    let rows: Vec<crate::LooperItem> = model.row_data(0).unwrap().loopers.iter().collect();
    assert_eq!(
        rows.len(),
        1,
        "the added looper must appear without a stream"
    );
    assert_eq!(rows[0].uid, 1);
    assert_eq!(rows[0].state_code, 0);
    assert!(
        !model.row_data(0).unwrap().looper_active,
        "an empty looper is not active"
    );
}
