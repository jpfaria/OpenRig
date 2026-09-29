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
