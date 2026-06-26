//! #716 RED: a rig input that references an I/O binding (`io_binding_ids`) MUST
//! project that reference onto the chain it becomes. The user's `project.openrig`
//! has `input-1` with `io_binding_ids: [io-1-1d68]`, yet activation reports
//! "chain 'rig:input-1' has no input blocks configured" — meaning the projected
//! chain reached the runtime WITHOUT its binding ids, so I/O could not be
//! discovered from the registry. Reproduce that the projection carries the ids.

const RIG_YAML: &str = r#"
version: 1
project:
  name: projectS
  inputs:
    input-1:
      label: guiTARRA - DEFAULT
      bank:
        1: guitarra-default
      active-preset: 1
      active-scene: 1
      routing: []
      instrument: electric_guitar
      io_binding_ids:
      - io-1-1d68
  outputs: {}
  presets:
    guitarra-default:
      id: guitarra-default
      name: guiTARRA - DEFAULT
      blocks:
      - id: chain:0:block:0
        enabled: true
        kind: !Core
          effect_type: gain
          model: volume
          params:
            values: {}
"#;

#[test]
fn rig_projection_carries_io_binding_ids_onto_the_chain() {
    let rig = infra_yaml::parse_rig_project(RIG_YAML).expect("parse rig");
    let chains = engine::rig_runtime::rig_to_chains(&rig);
    let chain = chains
        .iter()
        .find(|c| c.id.0 == "rig:input-1")
        .expect("rig must project a chain for input-1");
    assert_eq!(
        chain.io_binding_ids,
        vec!["io-1-1d68".to_string()],
        "the rig input's binding reference must reach the chain so I/O is \
         discovered from the registry (else: 'no input blocks configured')"
    );
}
