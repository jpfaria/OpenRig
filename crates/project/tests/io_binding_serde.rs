use domain::ids::DeviceId;
use project::io_binding::{IoBinding, IoEndpoint};

#[test]
fn io_binding_round_trips_through_yaml() {
    let binding = IoBinding {
        id: "main".into(),
        name: "Scarlett".into(),
        inputs: vec![IoEndpoint {
            name: "In1".into(),
            device_id: DeviceId("dev:in".into()),
            mode: Default::default(),
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out1".into(),
            device_id: DeviceId("dev:out".into()),
            mode: Default::default(),
            channels: vec![0, 1],
        }],
    };
    let yaml = serde_yaml::to_string(&binding).unwrap();
    let back: IoBinding = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(binding, back);
    assert!(yaml.contains("id: main"));
    assert!(yaml.contains("name: In1"));
}
