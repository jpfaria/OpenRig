//! #85 — the port editor offers the two choices a port actually has.

use domain::ids::DeviceId;
use domain::io_binding::ChannelMode;
use infra_filesystem::{IoBinding, IoEndpoint};

use super::{binding_options, endpoint_options};

fn endpoint(name: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "io-1".into(),
            name: "SCARLET".into(),
            inputs: vec![endpoint("In 1"), endpoint("In 2")],
            outputs: vec![endpoint("Out 1")],
        },
        IoBinding {
            id: "io-2".into(),
            name: String::new(),
            inputs: vec![],
            outputs: vec![endpoint("Aux")],
        },
    ]
}

#[test]
fn every_binding_is_offered_by_name() {
    let options = binding_options(&registry());
    assert_eq!(options[0].as_str(), "SCARLET");
    // An unnamed binding falls back to its id so the row is never blank.
    assert_eq!(options[1].as_str(), "io-2");
}

#[test]
fn an_input_port_lists_the_bindings_inputs() {
    let registry = registry();
    let options = endpoint_options(registry.first(), true);
    let names: Vec<&str> = options.iter().map(|s| s.as_str()).collect();
    assert_eq!(names, vec!["In 1", "In 2"]);
}

#[test]
fn an_output_port_lists_the_bindings_outputs() {
    let registry = registry();
    let options = endpoint_options(registry.first(), false);
    let names: Vec<&str> = options.iter().map(|s| s.as_str()).collect();
    assert_eq!(names, vec!["Out 1"]);
}

#[test]
fn no_binding_selected_means_no_endpoints() {
    assert!(endpoint_options(None, true).is_empty());
}
