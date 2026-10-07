//! Which saved settings mark a device row as chosen: the open project's, or,
//! with no project (the first-run wizard), the per-machine config's.

use super::saved_device_settings;
use infra_filesystem::{AppConfig, GuiAudioDeviceSettings};
use project::device::DeviceSettings;

fn gui(id: &str) -> GuiAudioDeviceSettings {
    crate::device_settings_resolve::default_device_settings(id.to_string(), id.to_string())
}

fn project(id: &str) -> DeviceSettings {
    crate::project_ops::build_device_settings_from_gui(&[gui(id)], &[])[0].clone()
}

#[test]
fn without_a_project_the_system_config_marks_the_rows() {
    let config = AppConfig {
        input_devices: vec![gui("in-a")],
        output_devices: vec![gui("out-b")],
        ..AppConfig::default()
    };
    let ids: Vec<String> = saved_device_settings(None, &config)
        .into_iter()
        .map(|s| s.device_id.0)
        .collect();
    assert_eq!(ids, vec!["in-a".to_string(), "out-b".to_string()]);
}

#[test]
fn without_a_project_or_saved_devices_nothing_is_marked() {
    assert!(saved_device_settings(None, &AppConfig::default()).is_empty());
}

#[test]
fn an_open_project_keeps_marking_the_rows() {
    let config = AppConfig {
        input_devices: vec![gui("in-a")],
        ..AppConfig::default()
    };
    let saved = [project("proj-x")];
    let ids: Vec<String> = saved_device_settings(Some(&saved), &config)
        .into_iter()
        .map(|s| s.device_id.0)
        .collect();
    assert_eq!(ids, vec!["proj-x".to_string()]);
}
