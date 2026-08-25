//! #881 — the E/S an insert points at must survive save + reopen. The editor
//! could not bind an insert at all before this issue, so nothing pinned that
//! the binding round-trips through the project file.

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;
use project::project::Project;
use tempfile::tempdir;

use super::*;

#[test]
fn insert_binding_roundtrips_through_the_project_file() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("insert.yaml");
    let repo = YamlProjectRepository { path: path.clone() };
    let project = Project {
        name: Some("insert".into()),
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:insert".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["main".into()],
            blocks: vec![AudioBlock {
                id: BlockId("insert:0".into()),
                enabled: true,
                kind: AudioBlockKind::Insert(project::block::InsertBlock {
                    model: "external_loop".into(),
                    io: "fx_loop".into(),
                }),
            }],
            di_output: None,
            loopers: Vec::new(),
        }],
        midi: None,
    };

    repo.save_project(&project).expect("save should succeed");
    let loaded = repo.load_current_project().expect("load should succeed");

    let AudioBlockKind::Insert(ref ib) = loaded.chains[0].blocks[0].kind else {
        panic!("the insert must come back as an Insert block");
    };
    assert_eq!(
        ib.io, "fx_loop",
        "#881: the E/S the user picked must survive save + reopen"
    );
}
