//! #324: the output a chain's DI plays to is project data, so it survives
//! save and reload of `project.yaml`.

use domain::ids::ChainId;
use project::chain::Chain;
use project::endpoint_ref::DiOutputRef;
use project::project::Project;

fn project_with_di_output(di_output: Option<DiOutputRef>) -> Project {
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
            io_binding_ids: vec!["main".into()],
            blocks: vec![],
            di_output,
            loopers: vec![],
            disabled_endpoints: Default::default(),
            mix: Default::default(),
        }],
    }
}

#[test]
fn di_output_survives_save_and_reload_of_project_yaml() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("project.yaml");
    let di_output = Some(DiOutputRef {
        binding_id: "monitors".into(),
        endpoint: "FRFR".into(),
    });
    let rig = project::migrate::migrate_legacy_project(&project_with_di_output(di_output.clone()));

    infra_yaml::save_project_file(&path, &rig).expect("save");
    let loaded = infra_yaml::load_project_file(&path).expect("load");

    let input = loaded.inputs.values().next().expect("one input");
    assert_eq!(input.di_output, di_output);
}
