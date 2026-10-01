//! #827, red-first: every chain's DI picker lists the saved takes — a chain
//! that is not running included. A DI-only chain is never enabled, so a
//! refresh that only visited running chains would never show it a take saved
//! after the project opened.

use std::path::PathBuf;

use application::di_loader::DiLoopSource;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;
use slint::{Model, ModelRc, SharedString, VecModel};

use super::apply_di_sources_to_rows;
use crate::ProjectChainItem;

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn rows(n: usize) -> VecModel<ProjectChainItem> {
    VecModel::from(
        (0..n)
            .map(|_| ProjectChainItem {
                di_loop_sources: ModelRc::new(VecModel::from(vec![SharedString::from(
                    crate::di_loop_ui_sources::CHOOSE_FILE_SENTINEL,
                )])),
                di_loop_selected_index: -1,
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    )
}

fn sources(model: &VecModel<ProjectChainItem>, idx: usize) -> Vec<String> {
    model
        .row_data(idx)
        .unwrap()
        .di_loop_sources
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn a_stopped_chain_lists_the_saved_takes() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain("rig:a"), chain("rig:b")],
        midi: None,
    };
    let model = rows(2);
    let takes = vec![PathBuf::from("/lib/looper-takes/riff.wav")];

    apply_di_sources_to_rows(&model, &project, |_| None, &[], &takes);

    for idx in 0..2 {
        assert_eq!(
            sources(&model, idx),
            vec!["riff.wav", crate::di_loop_ui_sources::CHOOSE_FILE_SENTINEL],
            "chain {idx} must offer the take"
        );
    }
}

#[test]
fn the_chain_playing_a_take_highlights_it() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain("rig:a"), chain("rig:b")],
        midi: None,
    };
    let model = rows(2);
    let take = PathBuf::from("/lib/looper-takes/riff.wav");
    let b = ChainId("rig:b".into());

    apply_di_sources_to_rows(
        &model,
        &project,
        |c| (*c == b).then(|| DiLoopSource::File(take.clone())),
        &["strat".to_string()],
        std::slice::from_ref(&take),
    );

    assert_eq!(model.row_data(0).unwrap().di_loop_selected_index, -1);
    assert_eq!(model.row_data(1).unwrap().di_loop_selected_index, 1);
}

#[test]
fn a_chain_without_a_row_yet_is_skipped() {
    // The project can gain a chain a tick before the row list follows it.
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain("rig:a"), chain("rig:b")],
        midi: None,
    };
    let model = rows(1);
    let takes = vec![PathBuf::from("/lib/looper-takes/riff.wav")];

    apply_di_sources_to_rows(&model, &project, |_| None, &[], &takes);

    assert_eq!(model.row_count(), 1, "no row is invented for rig:b");
    assert_eq!(
        sources(&model, 0),
        vec!["riff.wav", crate::di_loop_ui_sources::CHOOSE_FILE_SENTINEL]
    );
}

#[test]
fn an_unchanged_row_is_not_rewritten() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain("rig:a")],
        midi: None,
    };
    let model = rows(1);
    let takes = vec![PathBuf::from("/lib/looper-takes/riff.wav")];
    apply_di_sources_to_rows(&model, &project, |_| None, &[], &takes);
    let first = model.row_data(0).unwrap().di_loop_sources;

    apply_di_sources_to_rows(&model, &project, |_| None, &[], &takes);

    assert!(
        first == model.row_data(0).unwrap().di_loop_sources,
        "the same list must keep the same model — a rewrite every tick \
         re-renders every picker"
    );
}

fn take_rows(model: &VecModel<ProjectChainItem>, idx: usize) -> Vec<bool> {
    model
        .row_data(idx)
        .unwrap()
        .di_loop_take_rows
        .iter()
        .collect()
}

#[test]
fn every_row_knows_which_entries_are_deletable_takes() {
    // The DI panel shows a trash only on a saved take.
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain("rig:a")],
        midi: None,
    };
    let model = rows(1);
    let takes = vec![PathBuf::from("/lib/looper-takes/riff.wav")];

    apply_di_sources_to_rows(&model, &project, |_| None, &["strat".to_string()], &takes);

    assert_eq!(take_rows(&model, 0), vec![false, true, false]);
}

#[test]
fn a_deleted_take_loses_its_row_and_its_trash() {
    // The next tick after a delete lists the library as it now is.
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain("rig:a")],
        midi: None,
    };
    let model = rows(1);
    let takes = vec![
        PathBuf::from("/lib/looper-takes/riff.wav"),
        PathBuf::from("/lib/looper-takes/verse.wav"),
    ];
    apply_di_sources_to_rows(&model, &project, |_| None, &[], &takes);

    apply_di_sources_to_rows(&model, &project, |_| None, &[], &takes[1..]);

    assert_eq!(
        sources(&model, 0),
        vec!["verse.wav", crate::di_loop_ui_sources::CHOOSE_FILE_SENTINEL]
    );
    assert_eq!(take_rows(&model, 0), vec![true, false]);
}
