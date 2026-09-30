//! #328 — the endpoint checklists are chain configuration (spec §1.3): they
//! live on the rig input in `project.openrig`, survive save + reload, need no
//! version bump, and a file written before them loads with every endpoint
//! checked.

use infra_yaml::{parse_rig_project, serialize_rig_project};
use project::endpoint_disables::{EndpointNode, EndpointRef};

const BOUND: &str = r#"
project:
  inputs:
    g:
      bank:
        1: p
      active-preset: 1
      io_binding_ids: [io]
  presets:
    p:
      blocks: []
"#;

fn r(endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: "io".into(),
        endpoint: endpoint.into(),
    }
}

#[test]
fn an_inputs_unchecked_endpoints_survive_save_and_reload() {
    let mut rig = parse_rig_project(BOUND).expect("a pre-#328 file loads");
    assert!(
        rig.inputs["g"].disabled_endpoints.is_empty(),
        "a file written before the checklist has every endpoint checked"
    );
    let untouched = serialize_rig_project(&rig).expect("serialize");
    assert!(
        !untouched.contains("disabled_endpoints"),
        "nothing unchecked, nothing written, got:\n{untouched}"
    );

    let input = rig.inputs.get_mut("g").expect("input g");
    input
        .disabled_endpoints
        .set_enabled(EndpointNode::Input, r("in 2"), false);
    input
        .disabled_endpoints
        .set_enabled(EndpointNode::PathBOutput, r("out L"), false);
    let yaml = serialize_rig_project(&rig).expect("serialize");
    assert!(
        yaml.contains("disabled_endpoints"),
        "the checklist is written, got:\n{yaml}"
    );
    assert!(
        yaml.contains("version: 1\n"),
        "the checklist needs no version bump, got:\n{yaml}"
    );

    let back = parse_rig_project(&yaml).expect("reload");
    assert_eq!(
        back.inputs["g"].disabled_endpoints, rig.inputs["g"].disabled_endpoints,
        "the checklist survives save and reload"
    );
}

#[test]
fn a_checklist_edit_changes_the_legacy_serialization_the_dirty_check_compares() {
    use domain::ids::ChainId;
    use infra_yaml::{serialize_project, YamlProjectRepository};
    use project::chain::Chain;
    use project::endpoint_disables::EndpointDisables;
    use project::project::Project;

    let project_with = |disabled_endpoints: EndpointDisables| Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            mix: Default::default(),
            id: ChainId("rig:g".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["io".into()],
            blocks: Vec::new(),
            di_output: None,
            loopers: Vec::new(),
            disabled_endpoints,
        }],
        midi: None,
    };
    let mut unchecked = EndpointDisables::default();
    unchecked.set_enabled(EndpointNode::Input, r("in 2"), false);

    let before = serialize_project(&project_with(EndpointDisables::default())).expect("serialize");
    let after = serialize_project(&project_with(unchecked.clone())).expect("serialize");
    assert_ne!(
        before, after,
        "the dirty fingerprint must see a checklist edit, or Save answers 'no changes'"
    );

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("p.yaml");
    std::fs::write(&path, &after).expect("write");
    let loaded = YamlProjectRepository { path }
        .load_current_project()
        .expect("load");
    assert_eq!(loaded.chains[0].disabled_endpoints, unchecked);
}
