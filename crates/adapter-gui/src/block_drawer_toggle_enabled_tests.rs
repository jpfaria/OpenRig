//! Red-first (#436 D-3): alternar enabled no drawer troca o bloco REAL
//! via `Command::ToggleBlockEnabled` no dispatcher compartilhado — não
//! por mutação de draft + persist na GUI. In-memory, sem Slint.

use super::apply_toggle_block_drawer_enabled;
use crate::state::ProjectSession;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use std::path::PathBuf;

fn core_block(enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId("b0".into()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: "analog_warm".to_string(),
            params: ParameterSet::default(),
        }),
    }
}

#[test]
fn toggling_drawer_enabled_flips_the_block_via_command() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("c0".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            blocks: vec![core_block(true)],
        }],
    };
    let session = ProjectSession::new(project, None, None, PathBuf::from("./presets"));

    apply_toggle_block_drawer_enabled(&session, 0, 0).expect("apply deve ok");

    assert!(
        !session.project.borrow().chains[0].blocks[0].enabled,
        "enabled tem que virar false via Command::ToggleBlockEnabled"
    );

    apply_toggle_block_drawer_enabled(&session, 0, 0).expect("apply deve ok");
    assert!(
        session.project.borrow().chains[0].blocks[0].enabled,
        "segundo toggle volta a true"
    );
}
