//! #716 RED: loading a legacy `.yaml` project must NOT generate a sibling
//! `.openrig` file. The user's project files stay `.yaml` — no `.openrig` is
//! allowed to appear on disk. (User directive: "NÃO QUERO QUE GERE .openrig".)

use std::fs;
use std::path::PathBuf;

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

fn scratch_dir() -> PathBuf {
    // Portable per-crate temp dir provided by cargo for integration tests —
    // never a machine-specific absolute path.
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("issue_716_no_openrig")
}

fn legacy_project() -> Project {
    Project {
        name: Some("p".to_string()),
        device_settings: Vec::new(),
        midi: None,
        chains: vec![Chain {
            id: ChainId("chain-0".into()),
            description: Some("Guitar".into()),
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: Vec::new(),
            blocks: vec![AudioBlock {
                id: BlockId("gain".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: ParameterSet::default(),
                }),
            }],
            di_output: None,
        }],
    }
}

#[test]
fn loading_a_legacy_yaml_does_not_generate_a_sibling_openrig() {
    let dir = scratch_dir();
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let yaml = dir.join("proj.yaml");
    let legacy_yaml = infra_yaml::serialize_project(&legacy_project()).expect("serialize legacy");
    fs::write(&yaml, legacy_yaml).expect("write legacy project");

    let _ = infra_yaml::load_project_any(&yaml).expect("load legacy project");

    let openrig = dir.join("proj.openrig");
    assert!(
        !openrig.exists(),
        "loading a .yaml must NOT create a .openrig sibling — the project stays .yaml"
    );
    let bak = dir.join("proj.yaml.bak");
    assert!(
        !bak.exists(),
        "loading must NOT create a .yaml.bak — migration is in memory only"
    );
    assert!(yaml.exists(), "the .yaml project file must still be present");
}
