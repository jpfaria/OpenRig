//! #913 — loading a preset file onto a chain.
//!
//! Three contracts, each with a shipped bug behind it: the chain keeps its own
//! I/O across the swap (#518 — wrapping it here too gave the chain two of
//! each), the loaded blocks get fresh ids (the same file on two chains must not
//! give them the same block ids), and the file the user picked names the active
//! preset (#510 — without it the combobox kept the old label).

use super::{load_preset_onto_chain, PresetLoadError};
use crate::state::ProjectSession;
use crate::ProjectChainItem;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::chain::Chain;
use project::project::Project;
use slint::VecModel;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// A preset carrying one effect block plus I/O the dispatcher must NOT take.
const PRESET_YAML: &str = r#"
id: clean
name: Clean
instrument: electric_guitar
blocks:
  - type: input
    enabled: true
    model: standard
    io: io-main
    endpoint: In 1
  - type: gain
    enabled: true
    model: volume
  - type: output
    enabled: true
    model: standard
    io: io-main
    endpoint: Out 1
"#;

fn preset_file(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, PRESET_YAML).expect("write preset");
    path
}

fn io_block(id: &str, input: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: if input {
            AudioBlockKind::Input(InputBlock {
                model: "standard".into(),
                io: "io-main".into(),
                endpoint: "In 1".into(),
            })
        } else {
            AudioBlockKind::Output(OutputBlock {
                model: "standard".into(),
                io: "io-main".into(),
                endpoint: "Out 1".into(),
            })
        },
    }
}

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec!["io-main".into()],
        blocks: vec![
            io_block("in", true),
            AudioBlock {
                id: BlockId("old-gain".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: Default::default(),
                }),
            },
            io_block("out", false),
        ],
        di_output: None,
        loopers: vec![],
    }
}

fn session(chains: Vec<Chain>) -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains,
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-preset-load-tests"),
    ))))
}

fn rows() -> Rc<VecModel<ProjectChainItem>> {
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    Rc::new(VecModel::from(Vec::<ProjectChainItem>::new()))
}

fn blocks_of(session: &Rc<RefCell<Option<ProjectSession>>>, index: usize) -> Vec<AudioBlock> {
    session.borrow().as_ref().unwrap().project.borrow().chains[index]
        .blocks
        .clone()
}

fn load(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    index: usize,
    path: &PathBuf,
) -> Result<ChainId, PresetLoadError> {
    load_preset_onto_chain(session, index, path, &rows(), &[], &[])
}

#[test]
fn the_chain_keeps_exactly_one_input_and_one_output_after_a_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = preset_file(&dir, "clean.yaml");
    let session = session(vec![chain("chain:0")]);

    load(&session, 0, &path).expect("load");

    let blocks = blocks_of(&session, 0);
    let inputs = blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Input(_)))
        .count();
    let outputs = blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
        .count();
    assert_eq!(inputs, 1, "#518: the preset's I/O must not be added on top");
    assert_eq!(outputs, 1);
}

#[test]
fn the_loaded_blocks_get_fresh_ids() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = preset_file(&dir, "clean.yaml");
    let session = session(vec![chain("chain:0"), chain("chain:1")]);

    load(&session, 0, &path).expect("load onto chain 0");
    load(&session, 1, &path).expect("load onto chain 1");

    let first: Vec<String> = blocks_of(&session, 0)
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Core(_)))
        .map(|b| b.id.0.clone())
        .collect();
    let second: Vec<String> = blocks_of(&session, 1)
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Core(_)))
        .map(|b| b.id.0.clone())
        .collect();
    assert!(!first.is_empty() && !second.is_empty());
    for id in &first {
        assert!(
            !second.contains(id),
            "the same file on two chains must not give them the same block id: {id}"
        );
    }
}

#[test]
fn loading_returns_the_chain_it_landed_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = preset_file(&dir, "clean.yaml");
    let session = session(vec![chain("chain:0"), chain("chain:1")]);
    assert_eq!(
        load(&session, 1, &path),
        Ok(ChainId("chain:1".into())),
        "the caller refreshes what is keyed on this id"
    );
}

#[test]
fn a_chain_index_that_no_longer_resolves_loads_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = preset_file(&dir, "clean.yaml");
    let session = session(vec![chain("chain:0")]);
    assert_eq!(load(&session, 7, &path), Err(PresetLoadError::Gone));
    assert_eq!(
        blocks_of(&session, 0).len(),
        3,
        "the untouched chain keeps its blocks"
    );
}

#[test]
fn loading_with_no_project_open_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = preset_file(&dir, "clean.yaml");
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    assert_eq!(load(&none, 0, &path), Err(PresetLoadError::Gone));
}

#[test]
fn a_file_that_is_not_a_preset_is_reported_and_changes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("garbage.yaml");
    std::fs::write(&path, "this: is: not: a: preset: {[}\n").expect("write");
    let session = session(vec![chain("chain:0")]);

    assert!(matches!(
        load(&session, 0, &path),
        Err(PresetLoadError::Unreadable(_))
    ));
    assert_eq!(blocks_of(&session, 0).len(), 3);
}

#[test]
fn a_file_that_is_not_there_is_reported() {
    let session = session(vec![chain("chain:0")]);
    let missing = PathBuf::from("/nonexistent/openrig-913/clean.yaml");
    assert!(matches!(
        load(&session, 0, &missing),
        Err(PresetLoadError::Unreadable(_))
    ));
}

#[test]
fn loading_republishes_the_chain_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = preset_file(&dir, "clean.yaml");
    let session = session(vec![chain("chain:0")]);
    let rows = rows();
    load_preset_onto_chain(&session, 0, &path, &rows, &[], &[]).expect("load");
    use slint::Model;
    assert_eq!(rows.row_count(), 1);
}
