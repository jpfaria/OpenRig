//! Red-first (#436 D-4): editar parâmetro na janela Block Editor muta o
//! bloco REAL via `Command::SetBlockParameter*` / `PickBlockParameterFile`
//! no dispatcher compartilhado. In-memory, sem Slint.

use super::{
    apply_pick_block_parameter_file, apply_select_block_parameter_option,
    apply_set_block_parameter_bool, apply_set_block_parameter_number,
    apply_set_block_parameter_text,
};
use crate::state::ProjectSession;
use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use std::path::PathBuf;

fn session_with_param(key: &str, value: ParameterValue) -> ProjectSession {
    let mut params = ParameterSet::default();
    params.insert(key, value);
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("c0".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            blocks: vec![AudioBlock {
                id: BlockId("b0".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "delay".to_string(),
                    model: "analog_warm".to_string(),
                    params,
                }),
            }],
        }],
    };
    ProjectSession::new(project, None, None, PathBuf::from("./presets"))
}

fn block_params(s: &ProjectSession) -> ParameterSet {
    match &s.project.borrow().chains[0].blocks[0].kind {
        AudioBlockKind::Core(cb) => cb.params.clone(),
        _ => panic!("core esperado"),
    }
}

#[test]
fn number_param_goes_via_command() {
    let s = session_with_param("time_ms", ParameterValue::Float(120.0));
    apply_set_block_parameter_number(&s, 0, 0, "time_ms", 240.0).unwrap();
    assert_eq!(block_params(&s).get_f32("time_ms"), Some(240.0));
}

#[test]
fn bool_param_goes_via_command() {
    let s = session_with_param("sync", ParameterValue::Bool(false));
    apply_set_block_parameter_bool(&s, 0, 0, "sync", true).unwrap();
    assert_eq!(block_params(&s).get_bool("sync"), Some(true));
}

#[test]
fn text_param_goes_via_command() {
    let s = session_with_param("label", ParameterValue::String("a".into()));
    apply_set_block_parameter_text(&s, 0, 0, "label", "b").unwrap();
    assert_eq!(block_params(&s).get_string("label"), Some("b"));
}

#[test]
fn option_param_goes_via_command() {
    let s = session_with_param("wave", ParameterValue::String("sine".into()));
    apply_select_block_parameter_option(&s, 0, 0, "wave", "triangle", 2).unwrap();
    assert_eq!(block_params(&s).get_string("wave"), Some("triangle"));
}

#[test]
fn pick_file_param_goes_via_command() {
    let s = session_with_param("ir_path", ParameterValue::String(String::new()));
    apply_pick_block_parameter_file(&s, 0, 0, "ir_path", PathBuf::from("/tmp/x.wav")).unwrap();
    assert_eq!(block_params(&s).get_string("ir_path"), Some("/tmp/x.wav"));
}
