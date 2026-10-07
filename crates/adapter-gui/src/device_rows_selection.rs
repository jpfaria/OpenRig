//! Responsibility: picks the saved device settings that mark device rows as chosen.
//!
//! An open project marks them with its own settings; with no project (the
//! first-run wizard) the per-machine config does.

use infra_filesystem::AppConfig;
use project::device::DeviceSettings;

pub(crate) fn saved_device_settings(
    project: Option<&[DeviceSettings]>,
    app_config: &AppConfig,
) -> Vec<DeviceSettings> {
    match project {
        Some(saved) => saved.to_vec(),
        None => crate::project_ops::build_device_settings_from_gui(
            &app_config.input_devices,
            &app_config.output_devices,
        ),
    }
}

#[cfg(test)]
#[path = "device_rows_selection_tests.rs"]
mod tests;
