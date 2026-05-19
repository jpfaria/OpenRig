//! Red-first (#436 D-1): provar que escolher um modelo no Block Editor
//! troca o bloco REAL via `Command::ReplaceBlockModel` no dispatcher
//! compartilhado — não por mutação de draft na GUI. In-memory, sem Slint.

use super::apply_choose_block_model;
use crate::project_view::block_model_picker_items;
use crate::state::ProjectSession;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use std::path::PathBuf;

fn delay_block(model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId("b0".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: model.to_string(),
            params: ParameterSet::default(),
        }),
    }
}

#[test]
fn choosing_a_model_replaces_the_block_via_command_not_the_draft() {
    let inst = "electric_guitar";
    let items = block_model_picker_items("delay", inst);
    assert!(
        items.len() >= 2,
        "preciso de >=2 modelos de delay p/ esse teste"
    );
    let first = items[0].model_id.to_string();
    let target = items[1].model_id.to_string();
    assert_ne!(first, target);

    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("c0".into()),
            description: None,
            instrument: inst.to_string(),
            enabled: false,
            volume: 100.0,
            blocks: vec![delay_block(&first)],
        }],
    };
    let session = ProjectSession::new(project, None, None, PathBuf::from("./presets"));

    apply_choose_block_model(&session, 0, 0, 1).expect("apply_choose_block_model deve ok");

    let proj = session.project.borrow();
    let AudioBlockKind::Core(cb) = &proj.chains[0].blocks[0].kind else {
        panic!("esperava bloco Core após ReplaceBlockModel");
    };
    assert_eq!(
        cb.model, target,
        "o modelo do bloco tem que mudar via Command::ReplaceBlockModel"
    );
}
