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
