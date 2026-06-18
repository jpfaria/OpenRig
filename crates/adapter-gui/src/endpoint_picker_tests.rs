use super::{endpoint_names_for_input_binding, endpoint_names_for_output_binding, IoBindingModel, IoEndpointModel};

// ── endpoint_picker_lists_only_selected_binding_endpoints (#716) ──────────────

fn make_binding(id: &str, inputs: &[&str], outputs: &[&str]) -> IoBindingModel {
    IoBindingModel {
        id: id.into(),
        name: id.into(),
        inputs: inputs
            .iter()
            .map(|n| IoEndpointModel {
                name: n.to_string(),
                device_label: String::new(),
                mode: String::new(),
                channels_label: String::new(),
            })
            .collect(),
        outputs: outputs
            .iter()
            .map(|n| IoEndpointModel {
                name: n.to_string(),
                device_label: String::new(),
                mode: String::new(),
                channels_label: String::new(),
            })
            .collect(),
    }
}

#[test]
fn endpoint_picker_lists_only_selected_binding_endpoints_for_input() {
    let binding = make_binding("main", &["In1", "In2"], &["Out1"]);
    let names = endpoint_names_for_input_binding(&binding);
    assert_eq!(names, vec!["In1", "In2"]);
}

#[test]
fn endpoint_picker_lists_only_selected_binding_endpoints_for_output() {
    let binding = make_binding("main", &["In1"], &["Out1", "Monitor"]);
    let names = endpoint_names_for_output_binding(&binding);
    assert_eq!(names, vec!["Out1", "Monitor"]);
}

#[test]
fn endpoint_picker_input_not_mixed_with_output() {
    let binding = make_binding("fx", &["Guitar"], &["Send", "Return"]);
    let input_names = endpoint_names_for_input_binding(&binding);
    let output_names = endpoint_names_for_output_binding(&binding);
    assert_eq!(input_names, vec!["Guitar"]);
    assert_eq!(output_names, vec!["Send", "Return"]);
    // Input picker must NOT contain output endpoint names
    assert!(
        !input_names.contains(&"Send".to_string()),
        "input picker must not list output endpoints"
    );
}

#[test]
fn endpoint_picker_empty_binding_returns_empty_list() {
    let binding = make_binding("empty", &[], &[]);
    assert!(endpoint_names_for_input_binding(&binding).is_empty());
    assert!(endpoint_names_for_output_binding(&binding).is_empty());
}
