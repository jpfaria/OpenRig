//! #716 RED: reopening a saved project must restore the chain's selected I/O
//! bindings in the editor checklist.
//!
//! Repro of the user's bug: open project TESTE → chain 1 → "configure chain":
//! the bindings come UNCHECKED even though a binding was selected and saved
//! previously. The persisted `io_binding_ids` no longer surface as selected
//! when the project is reopened.

use adapter_gui::chain_binding_choices::binding_choices;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

#[test]
fn reopened_project_restores_selected_binding_in_checklist() {
    // Per-machine registry as it lives in config.yaml (binding id "main").
    let registry = vec![IoBinding {
        id: "main".into(),
        name: "Scarlett 2i2".into(),
        inputs: vec![IoEndpoint {
            name: "In 1".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out 1".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];

    // The chain the user configured + saved: it references binding "main".
    let chain = Chain {
        id: ChainId("rig:input-1".into()),
        description: Some("test".into()),
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };

    // Simulate save + reopen: round-trip the chain through YAML (the project
    // persistence format).
    let yaml = serde_yaml::to_string(&chain).expect("serialize chain");
    let reloaded: Chain = serde_yaml::from_str(&yaml).expect("deserialize chain");

    // Open the editor: build the checklist for the reloaded chain.
    let choices = binding_choices(&registry, &reloaded.io_binding_ids);

    let main = choices
        .iter()
        .find(|c| c.id.as_str() == "main")
        .expect("binding 'main' row must be present in the checklist");
    assert!(
        main.selected,
        "reopened chain must show binding 'main' as selected; \
         reloaded io_binding_ids = {:?}, serialized yaml =\n{}",
        reloaded.io_binding_ids, yaml
    );
}
