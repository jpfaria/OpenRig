//! #716: pure helpers that bridge the chain editor's binding checklist and the
//! domain. `binding_choices` builds the checklist model (every registry binding,
//! marked selected when the chain references it); `selected_binding_ids` reads
//! the checked ids back out (in registry order) for the saved chain.

use adapter_gui::chain_binding_choices::{binding_choices, selected_binding_ids};
use adapter_gui::ChainBindingChoice;
use domain::ids::DeviceId;
use domain::io_binding::{IoBinding, IoEndpoint};

fn binding(id: &str, name: &str) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: name.into(),
        inputs: vec![],
        outputs: vec![],
    }
}

#[test]
fn binding_choices_marks_the_chains_selected_bindings() {
    let registry = vec![
        binding("xyz", "XYZ"),
        binding("abc", "ABC"),
        binding("def", "DEF"),
    ];
    let choices = binding_choices(&registry, &["abc".to_string()]);

    assert_eq!(choices.len(), 3, "every registry binding becomes a row");
    assert_eq!(choices[0].id, "xyz");
    assert!(!choices[0].selected, "xyz not referenced");
    assert_eq!(choices[1].id, "abc");
    assert_eq!(choices[1].name, "ABC");
    assert!(choices[1].selected, "abc is referenced by the chain");
    assert!(!choices[2].selected);
}

#[test]
fn selected_binding_ids_reads_checked_rows_in_order() {
    let choices = vec![
        ChainBindingChoice {
            id: "xyz".into(),
            name: "XYZ".into(),
            selected: true,
            ..Default::default()
        },
        ChainBindingChoice {
            id: "abc".into(),
            name: "ABC".into(),
            selected: false,
            ..Default::default()
        },
        ChainBindingChoice {
            id: "def".into(),
            name: "DEF".into(),
            selected: true,
            ..Default::default()
        },
    ];
    assert_eq!(
        selected_binding_ids(&choices),
        vec!["xyz".to_string(), "def".to_string()],
        "only checked rows, in order"
    );
}

#[test]
fn binding_choices_label_the_pills_with_one_based_channels() {
    let endpoint = |channels: Vec<usize>| IoEndpoint {
        name: "e".into(),
        device_id: DeviceId("dev".into()),
        mode: Default::default(),
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "Main".into(),
            inputs: vec![endpoint(vec![0])],
            outputs: vec![endpoint(vec![0, 1])],
        },
        binding("none", "None"),
    ];
    let choices = binding_choices(&registry, &[]);
    assert_eq!(choices[0].inputs_label, "1");
    assert_eq!(choices[0].outputs_label, "1,2");
    assert_eq!(choices[1].inputs_label, "", "no inputs, no IN pill");
    assert_eq!(choices[1].outputs_label, "");
}
