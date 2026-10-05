//! The presence check the chain toggle asks before switching on.
//!
//! A device id the host does not list is absent, in both directions. Only a
//! read of the host's device list: no stream is opened and nothing changes.

use domain::ids::DeviceId;
use project::binding_discovery::PortDirection;

#[test]
fn a_device_id_the_host_does_not_list_is_absent() {
    let missing = DeviceId("openrig-1069-no-such-device".to_string());

    assert!(!infra_cpal::device_is_present(
        &missing,
        PortDirection::Input
    ));
    assert!(!infra_cpal::device_is_present(
        &missing,
        PortDirection::Output
    ));
}
